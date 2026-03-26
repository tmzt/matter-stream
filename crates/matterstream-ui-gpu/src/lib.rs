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

use matterstream_common::SdfDrawCmd;

/// GPU SDF renderer. Holds pipeline state and buffers.
/// Does NOT own the wgpu device — attaches to an existing context.
/// GPU Anim struct matching WGSL layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuAnim {
    freq: f32,
    duty: f32,
    enable_ref: u32,
    _pad: u32,
}

pub struct GpuSdfRenderer {
    pipeline: wgpu::RenderPipeline,
    draw_cmd_buffer: wgpu::Buffer,
    header_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    anim_buffer: wgpu::Buffer,
    glyph_bitmap_buffer: wgpu::Buffer,
    char_buffer_gpu: wgpu::Buffer,
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

/// Minimal uniforms for the shader. Caller can extend.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MinimalUniforms {
    time_delta: [f32; 4],
    resolution: [f32; 4],
    mouse: [f32; 4],
    theme: [f32; 4],
    // Tier 1 banks (zeroed — not used for basic SDF rendering)
    vec4_bank: [[f32; 4]; 16],
    vec3_bank: [[f32; 4]; 16],
    scalar_bank: [[f32; 4]; 4],
    int_bank: [[i32; 4]; 4],
    zero_page: [[u32; 4]; 16],
    font: [u32; 4],  // [glyph_w, glyph_h, first_cp, last_cp]
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
const SHADER_SOURCE: &str = include_str!("shader_render.wgsl");

impl GpuSdfRenderer {
    /// Create a new renderer attached to an existing wgpu device.
    /// Does NOT take ownership of the device.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sdf_render"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        // Buffers
        let draw_cmd_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw_cmds"),
            size: (MAX_DRAW_CMDS as u64) * std::mem::size_of::<GpuDrawCmd>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let header_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render_header"),
            size: 16,
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
            label: Some("glyph_bitmap"),
            size: 8192, // enough for 5x8 font × 95 glyphs = 760 u32s
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let char_buffer_gpu = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("char_buffer"),
            size: 16384,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Bind group layout
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
            bind_group,
            max_cmds: MAX_DRAW_CMDS,
        }
    }

    /// Render SdfDrawCmd list to a texture view.
    /// The caller owns the surface/texture — this just records render commands.
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
        // Font descriptor goes in uniforms (uploaded each frame via render_full_scaled)
        // Bitmap goes in storage buffer
        if !bitmap.is_empty() {
            queue.write_buffer(&self.glyph_bitmap_buffer, 0, bytemuck::cast_slice(bitmap));
        }
        let _ = font; // stored in uniforms, uploaded per-frame
    }

    /// Upload character data for text rendering.
    pub fn upload_chars(&self, queue: &wgpu::Queue, chars: &[u32]) {
        if !chars.is_empty() {
            queue.write_buffer(&self.char_buffer_gpu, 0, bytemuck::cast_slice(chars));
        }
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

        // Upload draw commands (convert to GPU-safe wrapper type)
        if count > 0 {
            let gpu_cmds: Vec<GpuDrawCmd> = draws[..count].iter().map(GpuDrawCmd::from).collect();
            queue.write_buffer(
                &self.draw_cmd_buffer,
                0,
                bytemuck::cast_slice(&gpu_cmds),
            );
        }

        // Upload header
        let header = RenderHeader {
            cmd_count: count as u32,
            _pad: [0; 3],
        };
        queue.write_buffer(&self.header_buffer, 0, bytemuck::bytes_of(&header));

        // Upload uniforms with bank values
        let mut uniforms = MinimalUniforms::default();
        uniforms.time_delta = [time_ms, 0.0, 0.0, 0.0];
        uniforms.resolution = [width as f32, height as f32, scale, 0.0];
        // Pack scalar_bank into vec4 groups
        for (i, val) in scalar_bank.iter().take(16).enumerate() {
            uniforms.scalar_bank[i / 4][i % 4] = *val;
        }
        // Pack int_bank into ivec4 groups
        for (i, val) in int_bank.iter().take(16).enumerate() {
            uniforms.int_bank[i / 4][i % 4] = *val;
        }
        if let Some(f) = font {
            uniforms.font = [f.glyph_w, f.glyph_h, f.first_cp, f.last_cp];
        }
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Upload anim bank
        if !anim_bank.is_empty() {
            let gpu_anims: Vec<GpuAnim> = anim_bank.iter().map(|a| GpuAnim {
                freq: a.freq, duty: a.duty, enable_ref: a.enable_ref, _pad: 0,
            }).collect();
            queue.write_buffer(&self.anim_buffer, 0, bytemuck::cast_slice(&gpu_anims));
        }

        // Render pass
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
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1, g: 0.1, b: 0.12, a: 1.0,
                        }),
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
            pass.draw(0..3, 0..1); // Full-screen triangle
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}
