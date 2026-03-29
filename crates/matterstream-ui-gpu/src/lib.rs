//! GPU SDF renderer for MatterStream UI.
//!
//! Accepts an existing wgpu Device/Queue (shared with ML models, etc.)
//! and renders SdfDrawCmd lists via a fragment shader SDF pipeline.
//!
//! The renderer does NOT own the device or surface — the caller manages those.
//!
//! ```ignore
//! let renderer = GpuSdfRenderer::new(&device, &queue, surface_format);
//! renderer.render(&device, &queue, &surface_view, width, height, &sdf_draws);
//! ```

use matterstream_common::{SdfDrawCmd, RenderFrame};

/// GPU Anim struct matching WGSL layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuAnim {
    freq: f32,
    duty: f32,
    enable_ref: u32,
    _pad: u32,
}

/// GPU texture descriptor matching WGSL layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuTextureDesc {
    width: u32,
    height: u32,
    layer: u32,
    flags: u32,
}

pub struct GpuSdfRenderer {
    pipeline: wgpu::RenderPipeline,
    draw_cmd_buffer: wgpu::Buffer,
    header_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    anim_buffer: wgpu::Buffer,
    glyph_bitmap_buffer: wgpu::Buffer,
    char_buffer_gpu: wgpu::Buffer,
    texture_bank_buffer: wgpu::Buffer,
    glyph_table_buffer: wgpu::Buffer,
    // Held for ownership — referenced via bind_group, not read through self
    #[allow(dead_code)]
    tex_array: wgpu::Texture,
    #[allow(dead_code)]
    tex_array_view: wgpu::TextureView,
    #[allow(dead_code)]
    tex_sampler: wgpu::Sampler,
    #[allow(dead_code)]
    msdf_atlas_texture: wgpu::Texture,
    #[allow(dead_code)]
    msdf_atlas_view: wgpu::TextureView,
    #[allow(dead_code)]
    msdf_sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    max_cmds: u32,
}

/// Header matching the WGSL RenderHeader struct.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RenderHeader {
    cmd_count: u32,
    _pad: [u32; 3],
}

/// Minimal uniforms for the shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MinimalUniforms {
    time_delta: [f32; 4],
    resolution: [f32; 4],
    mouse: [f32; 4],
    theme: [f32; 4],
    vec4_bank: [[f32; 4]; 16],
    vec3_bank: [[f32; 4]; 16],
    scalar_bank: [[f32; 4]; 4],
    int_bank: [[i32; 4]; 4],
    zero_page: [[u32; 4]; 16],
    font: [u32; 4],
}

impl Default for MinimalUniforms {
    fn default() -> Self {
        Self {
            time_delta: [0.0; 4],
            resolution: [400.0, 300.0, 1.0, 0.0],
            mouse: [0.0; 4],
            theme: [1.0, 0.0, 0.0, 0.0],
            vec4_bank: [[0.0; 4]; 16],
            vec3_bank: [[0.0; 4]; 16],
            scalar_bank: [[0.0; 4]; 4],
            int_bank: [[0; 4]; 4],
            zero_page: [[0; 4]; 16],
            font: [0; 4],
        }
    }
}

// SdfDrawCmd bytemuck wrapper — can't impl foreign trait on foreign type
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuDrawCmd {
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    params: [f32; 4],
}

impl From<&SdfDrawCmd> for GpuDrawCmd {
    fn from(cmd: &SdfDrawCmd) -> Self {
        Self { pos: cmd.pos, size: cmd.size, color: cmd.color, params: cmd.params }
    }
}

const MAX_DRAW_CMDS: u32 = 4096;
const MAX_TEXTURE_LAYERS: u32 = 8;
const MAX_GLYPH_TABLE_ENTRIES: u32 = 4096;
const PLACEHOLDER_TEX_SIZE: u32 = 1;
const SHADER_SOURCE: &str = include_str!("shader_render.wgsl");

fn make_storage_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl GpuSdfRenderer {
    /// Create a new renderer attached to an existing wgpu device.
    /// Does NOT take ownership of the device.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        Self::new_with_msdf(device, surface_format, 1, 1)
    }

    /// Create a renderer with a pre-sized MSDF atlas texture.
    pub fn new_with_msdf(device: &wgpu::Device, surface_format: wgpu::TextureFormat, msdf_width: u32, msdf_height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sdf_render"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        let draw_cmd_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw_cmds"),
            size: (MAX_DRAW_CMDS as u64) * std::mem::size_of::<GpuDrawCmd>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let header_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render_header"), size: 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<MinimalUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let anim_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("anim_bank"),
            size: (matterstream_common::MAX_ANIMS as u64) * std::mem::size_of::<GpuAnim>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let glyph_bitmap_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_bitmap"), size: 8192,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let char_buffer_gpu = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("char_buffer"), size: 16384,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let texture_bank_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture_bank"),
            size: (MAX_TEXTURE_LAYERS as u64) * std::mem::size_of::<GpuTextureDesc>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let glyph_table_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_table"),
            size: (MAX_GLYPH_TABLE_ENTRIES as u64) * 32,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tex_array = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tex_array"),
            size: wgpu::Extent3d { width: PLACEHOLDER_TEX_SIZE, height: PLACEHOLDER_TEX_SIZE, depth_or_array_layers: MAX_TEXTURE_LAYERS },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let tex_array_view = tex_array.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let tex_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tex_sampler"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });

        let msdf_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msdf_atlas"),
            size: wgpu::Extent3d { width: msdf_width.max(1), height: msdf_height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let msdf_atlas_view = msdf_atlas_texture.create_view(&Default::default());
        let msdf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("msdf_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Bind group layout (bindings 0-11)
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sdf_render_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                make_storage_layout(1),
                make_storage_layout(2),
                make_storage_layout(3),
                make_storage_layout(4),
                make_storage_layout(5),
                // Binding 6: texture_2d_array for texture bank
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 7: sampler for texture bank
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Binding 8: texture_bank descriptors
                make_storage_layout(8),
                // Binding 9: MSDF atlas texture
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 10: MSDF sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Binding 11: glyph table
                make_storage_layout(11),
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sdf_render_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: draw_cmd_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: header_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: anim_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: glyph_bitmap_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: char_buffer_gpu.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&tex_array_view) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&tex_sampler) },
                wgpu::BindGroupEntry { binding: 8, resource: texture_bank_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(&msdf_atlas_view) },
                wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::Sampler(&msdf_sampler) },
                wgpu::BindGroupEntry { binding: 11, resource: glyph_table_buffer.as_entire_binding() },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sdf_render_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sdf_render_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            draw_cmd_buffer,
            header_buffer,
            uniform_buffer,
            anim_buffer,
            glyph_bitmap_buffer,
            char_buffer_gpu,
            texture_bank_buffer,
            glyph_table_buffer,
            tex_array,
            tex_array_view,
            tex_sampler,
            msdf_atlas_texture,
            msdf_atlas_view,
            msdf_sampler,
            bind_group,
            max_cmds: MAX_DRAW_CMDS,
        }
    }

    /// Render SdfDrawCmd list to a texture view.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        draws: &[SdfDrawCmd],
    ) {
        self.render_animated(device, queue, target, width, height, draws, 0.0);
    }

    pub fn render_animated(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
    ) {
        self.render_full(device, queue, target, width, height, draws, time_ms, &[0.0; 16], &[0; 16], &[], None);
    }

    pub fn render_full(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        scalar_bank: &[f32],
        int_bank: &[i32],
        anim_bank: &[matterstream_common::Anim],
        font: Option<&matterstream_common::GpuFont>,
    ) {
        self.render_full_scaled(device, queue, target, width, height, 1.0, draws, time_ms, scalar_bank, int_bank, anim_bank, font);
    }

    /// Upload font atlas data (call once or when font changes).
    pub fn upload_font(&self, queue: &wgpu::Queue, font: &matterstream_common::GpuFont, bitmap: &[u32]) {
        if !bitmap.is_empty() {
            queue.write_buffer(&self.glyph_bitmap_buffer, 0, bytemuck::cast_slice(bitmap));
        }
        let _ = font;
    }

    /// Upload character data for text rendering.
    pub fn upload_chars(&self, queue: &wgpu::Queue, chars: &[u32]) {
        if !chars.is_empty() {
            queue.write_buffer(&self.char_buffer_gpu, 0, bytemuck::cast_slice(chars));
        }
    }

    /// Upload MSDF glyph table (array of u32, 8 per entry = 2 × vec4<u32>).
    pub fn upload_glyph_table(&self, queue: &wgpu::Queue, table: &[u32]) {
        if !table.is_empty() {
            queue.write_buffer(&self.glyph_table_buffer, 0, bytemuck::cast_slice(table));
        }
    }

    /// Upload MSDF atlas RGBA pixel data to the atlas texture.
    /// Data must be RGBA8 (4 bytes per pixel), width × height × 4 bytes.
    /// Note: the atlas texture is initialized as 1x1; this writes into it.
    /// For atlases larger than 1x1, call `new_with_msdf_atlas_size` or
    /// write to the texture directly.
    pub fn upload_msdf_atlas(&self, queue: &wgpu::Queue, width: u32, height: u32, rgba_data: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.msdf_atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }

    pub fn render_full_scaled(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        scalar_bank: &[f32],
        int_bank: &[i32],
        anim_bank: &[matterstream_common::Anim],
        font: Option<&matterstream_common::GpuFont>,
    ) {
        let count = draws.len().min(self.max_cmds as usize);

        if count > 0 {
            let gpu_cmds: Vec<GpuDrawCmd> = draws[..count].iter().map(GpuDrawCmd::from).collect();
            queue.write_buffer(&self.draw_cmd_buffer, 0, bytemuck::cast_slice(&gpu_cmds));
        }

        let header = RenderHeader { cmd_count: count as u32, _pad: [0; 3] };
        queue.write_buffer(&self.header_buffer, 0, bytemuck::bytes_of(&header));

        let mut uniforms = MinimalUniforms::default();
        uniforms.time_delta = [time_ms, 0.0, 0.0, 0.0];
        uniforms.resolution = [width as f32, height as f32, scale, 0.0];
        for (i, val) in scalar_bank.iter().take(16).enumerate() {
            uniforms.scalar_bank[i / 4][i % 4] = *val;
        }
        for (i, val) in int_bank.iter().take(16).enumerate() {
            uniforms.int_bank[i / 4][i % 4] = *val;
        }
        if let Some(f) = font {
            uniforms.font = [f.glyph_w, f.glyph_h, f.first_cp, f.last_cp];
        }
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        if !anim_bank.is_empty() {
            let gpu_anims: Vec<GpuAnim> = anim_bank.iter().map(|a| GpuAnim {
                freq: a.freq, duty: a.duty, enable_ref: a.enable_ref, _pad: 0,
            }).collect();
            queue.write_buffer(&self.anim_buffer, 0, bytemuck::cast_slice(&gpu_anims));
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sdf_render_encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sdf_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.12, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a fully prepared `RenderFrame`.
    pub fn render_frame(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        frame: &RenderFrame,
    ) {
        if !frame.char_buffer.is_empty() {
            queue.write_buffer(&self.char_buffer_gpu, 0, bytemuck::cast_slice(&frame.char_buffer));
        }
        if !frame.glyph_bitmap.is_empty() {
            queue.write_buffer(&self.glyph_bitmap_buffer, 0, bytemuck::cast_slice(&frame.glyph_bitmap));
        }

        // Upload texture_bank descriptors
        if !frame.texture_bank.is_empty() {
            let gpu_descs: Vec<GpuTextureDesc> = frame.texture_bank.iter().map(|t| GpuTextureDesc {
                width: t.width, height: t.height, layer: t.layer, flags: t.flags,
            }).collect();
            queue.write_buffer(&self.texture_bank_buffer, 0, bytemuck::cast_slice(&gpu_descs));
        }

        self.render_full_scaled(
            device, queue, target,
            frame.width, frame.height, frame.scale,
            &frame.draws, frame.time_ms,
            &frame.scalar_bank, &frame.int_bank,
            &frame.anim_bank, Some(&frame.font),
        );
    }
}
