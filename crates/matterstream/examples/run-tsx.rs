//! Example: Load a .tsx file, compile it, and render in a native window with GPU.
//!
//! Run with:
//!   cargo run -p matterstream --example run-tsx -- crates/matterstream/examples/example.tsx
//!   cargo run -p matterstream --example run-tsx -- crates/matterstream/examples/login_form.tsx
//!   cargo run -p matterstream --example run-tsx -- --timeout 5 crates/matterstream/examples/login_form.tsx

use std::env;
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use matterstream::{Compiler, MatterStream, OpsHeader, Parser, Primitive, RsiPointer, BankId};
use wgpu::util::DeviceExt;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

use glyphon::{
    Attrs, Buffer as TextBuffer, Cache, Color as TextColor, Family, FontSystem, Metrics,
    Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

// ---------------------------------------------------------------------------
// GPU data types
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct SlabInstance {
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

const QUAD_VERTICES: &[Vertex] = &[
    Vertex { position: [0.0, 0.0] },
    Vertex { position: [1.0, 0.0] },
    Vertex { position: [1.0, 1.0] },
    Vertex { position: [0.0, 1.0] },
];

const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

const SHADER: &str = "
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) vertex_pos: vec2<f32>,
    @location(1) inst_position: vec2<f32>,
    @location(2) inst_size: vec2<f32>,
    @location(3) inst_color: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    let pos = (vertex_pos - vec2(0.5, 0.5)) * inst_size + inst_position;
    out.clip_position = vec4(pos.x, pos.y, 0.0, 1.0);
    out.color = inst_color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
";

fn main() {
    // ── Arg parsing ──────────────────────────────────────────────
    let args: Vec<String> = env::args().collect();
    let mut timeout_s = None;
    let mut file_path = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout" => {
                if i + 1 < args.len() {
                    timeout_s = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    eprintln!("--timeout requires a value");
                    return;
                }
            }
            _ => {
                if args[i].starts_with('-') {
                    eprintln!("Unknown flag: {}", args[i]);
                } else {
                    file_path = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }

    if let Some(seconds) = timeout_s {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(seconds));
            println!("Timeout reached, exiting.");
            std::process::exit(0);
        });
    }

    let file_path = if let Some(path) = file_path {
        path
    } else {
        eprintln!(
            "Usage: cargo run -p matterstream --example run-tsx -- [--timeout <seconds>] <file.tsx>"
        );
        return;
    };

    // ── Load & compile ───────────────────────────────────────────
    let code = match fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading tsx file '{}': {}", file_path, e);
            return;
        }
    };

    let _parsed = match Parser::parse(&code) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error parsing tsx file: {}", e);
            return;
        }
    };

    let compiled = match Compiler::compile(&code) {
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("Error compiling tsx file: {}", e);
            return;
        }
    };

    println!("Compiled {} ops from {}", compiled.ops.len(), file_path);

    // ── Window ───────────────────────────────────────────────────
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title(format!("MatterStream — {}", file_path))
                    .with_inner_size(winit::dpi::LogicalSize::new(640, 480)),
            )
            .unwrap(),
    );

    // ── wgpu setup ───────────────────────────────────────────────
    let instance = wgpu::Instance::default();
    let surface = instance.create_surface(window.clone()).unwrap();

    let adapter = pollster::block_on(
        instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }),
    )
    .expect("No suitable GPU adapter found");

    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor::default()),
    )
    .expect("Failed to create device");

    let size = window.inner_size();
    let mut config = surface
        .get_default_config(&adapter, size.width.max(1), size.height.max(1))
        .unwrap();
    surface.configure(&device, &config);

    // ── Slab render pipeline ─────────────────────────────────────
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("slab_shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x2,
        }],
    };

    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<SlabInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 8,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("slab_pipeline_layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let slab_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("slab_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[vertex_layout, instance_layout],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
        multiview: None,
        cache: None,
    });

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("quad_vertices"),
        contents: bytemuck::cast_slice(QUAD_VERTICES),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("quad_indices"),
        contents: bytemuck::cast_slice(QUAD_INDICES),
        usage: wgpu::BufferUsages::INDEX,
    });

    // ── Glyphon text setup ───────────────────────────────────────
    let mut font_system = FontSystem::new();
    let mut swash_cache = SwashCache::new();
    let glyphon_cache = Cache::new(&device);
    let mut text_atlas = TextAtlas::new(&device, &queue, &glyphon_cache, config.format);
    let mut text_renderer =
        TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);
    let mut viewport = Viewport::new(&device, &glyphon_cache);

    // ── Execute & render loop ────────────────────────────────────
    let scale_factor = window.scale_factor() as f32;
    let mut stream = MatterStream::new();
    let header = OpsHeader::new(vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)], false);

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent {
                    event: WindowEvent::Resized(new_size),
                    ..
                } => {
                    config.width = new_size.width.max(1);
                    config.height = new_size.height.max(1);
                    surface.configure(&device, &config);
                    window.request_redraw();
                }
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    let width = config.width;
                    let height = config.height;
                    if width == 0 || height == 0 {
                        return;
                    }

                    // Execute ops
                    smol::block_on(async {
                        let _ = stream.execute(&header, &compiled.ops).await;
                    });

                    // ── Pass 1: Measure text for auto-sizing ─────
                    let font_size = (height as f32 * 0.035).max(14.0);
                    let metrics = Metrics::new(font_size, font_size * 1.2);

                    // Measure text dimensions for each draw with a label
                    let mut text_measurements: Vec<Option<(f32, f32)>> = Vec::new();
                    for draw in &stream.draws {
                        if let Some(label) = &draw.label {
                            let mut buffer = TextBuffer::new(&mut font_system, metrics);
                            buffer.set_size(
                                &mut font_system,
                                Some(width as f32),
                                Some(font_size * 2.0),
                            );
                            buffer.set_text(
                                &mut font_system,
                                label,
                                &Attrs::new().family(Family::SansSerif),
                                Shaping::Advanced,
                            );
                            buffer.shape_until_scroll(&mut font_system, false);

                            let text_w: f32 =
                                buffer.layout_runs().map(|run| run.line_w).sum();
                            let line_h = font_size * 1.2;
                            text_measurements.push(Some((text_w, line_h)));
                        } else {
                            text_measurements.push(None);
                        }
                    }

                    // ── Pass 2: Build slab instances ────────────
                    let mut slab_instances: Vec<SlabInstance> = Vec::new();

                    for (i, draw) in stream.draws.iter().enumerate() {
                        let is_text_only = draw.primitive == Primitive::Text
                            && draw.size[0] == 0.0
                            && draw.size[1] == 0.0;

                        if !is_text_only {
                            let has_padding = draw.padding.iter().any(|p| *p > 0.0);
                            let has_explicit_size = draw.size[0] > 0.0 || draw.size[1] > 0.0;

                            let size = if !has_explicit_size && has_padding {
                                // Auto-size from text metrics + padding
                                // Padding is in logical pixels; scale to physical
                                if let Some(Some((text_w, text_h))) = text_measurements.get(i) {
                                    let pad_t = draw.padding[0] * scale_factor;
                                    let pad_r = draw.padding[1] * scale_factor;
                                    let pad_b = draw.padding[2] * scale_factor;
                                    let pad_l = draw.padding[3] * scale_factor;
                                    let slab_w_px = text_w + pad_r + pad_l;
                                    let slab_h_px = text_h + pad_t + pad_b;
                                    let w_ndc = slab_w_px / (width as f32 / 2.0);
                                    let h_ndc = slab_h_px / (height as f32 / 2.0);
                                    [w_ndc, h_ndc]
                                } else {
                                    [0.6, 0.1]
                                }
                            } else if has_explicit_size {
                                [draw.size[0], draw.size[1]]
                            } else {
                                [0.6, 0.1] // Default slab size
                            };
                            slab_instances.push(SlabInstance {
                                position: [draw.position[0], draw.position[1]],
                                size,
                                color: draw.color,
                            });
                        }
                    }

                    // ── Build text areas ─────────────────────────
                    let mut text_buffers: Vec<TextBuffer> = Vec::new();
                    let mut text_meta: Vec<(f32, f32, TextColor)> = Vec::new();

                    for draw in &stream.draws {
                        if let Some(label) = &draw.label {
                            let mut buffer = TextBuffer::new(&mut font_system, metrics);
                            buffer.set_size(
                                &mut font_system,
                                Some(width as f32),
                                Some(font_size * 2.0),
                            );
                            buffer.set_text(
                                &mut font_system,
                                label,
                                &Attrs::new().family(Family::SansSerif),
                                Shaping::Advanced,
                            );
                            buffer.shape_until_scroll(&mut font_system, false);

                            // NDC to screen pixels
                            let px =
                                (draw.position[0] + 1.0) / 2.0 * width as f32;
                            let py =
                                (-draw.position[1] + 1.0) / 2.0 * height as f32;

                            // Center text
                            let text_w: f32 =
                                buffer.layout_runs().map(|run| run.line_w).sum();
                            let left = px - text_w / 2.0;
                            let top = py - font_size / 2.0;

                            // Use text_color if present (nested Text in Slab),
                            // otherwise use draw.color for standalone Text primitives
                            let is_text_only = draw.primitive == Primitive::Text
                                && draw.size[0] == 0.0
                                && draw.size[1] == 0.0;

                            let color = if let Some(tc) = draw.text_color {
                                TextColor::rgba(
                                    (tc[0] * 255.0) as u8,
                                    (tc[1] * 255.0) as u8,
                                    (tc[2] * 255.0) as u8,
                                    (tc[3] * 255.0) as u8,
                                )
                            } else if is_text_only {
                                TextColor::rgba(
                                    (draw.color[0] * 255.0) as u8,
                                    (draw.color[1] * 255.0) as u8,
                                    (draw.color[2] * 255.0) as u8,
                                    (draw.color[3] * 255.0) as u8,
                                )
                            } else {
                                TextColor::rgba(255, 255, 255, 255)
                            };

                            text_buffers.push(buffer);
                            text_meta.push((left, top, color));
                        }
                    }

                    let text_areas: Vec<TextArea> = text_buffers
                        .iter()
                        .zip(text_meta.iter())
                        .map(|(buffer, (left, top, color))| TextArea {
                            buffer,
                            left: *left,
                            top: *top,
                            scale: 1.0,
                            bounds: TextBounds {
                                left: 0,
                                top: 0,
                                right: width as i32,
                                bottom: height as i32,
                            },
                            default_color: *color,
                            custom_glyphs: &[],
                        })
                        .collect();

                    viewport.update(&queue, Resolution { width, height });

                    text_renderer
                        .prepare(
                            &device,
                            &queue,
                            &mut font_system,
                            &mut text_atlas,
                            &viewport,
                            text_areas,
                            &mut swash_cache,
                        )
                        .unwrap();

                    // ── GPU render ────────────────────────────────
                    let frame = match surface.get_current_texture() {
                        Ok(f) => f,
                        Err(_) => return,
                    };
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("render_encoder"),
                        });

                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("main_pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.09,
                                        g: 0.09,
                                        b: 0.11,
                                        a: 1.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            ..Default::default()
                        });

                        // Draw slabs
                        if !slab_instances.is_empty() {
                            let instance_buffer =
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("instance_buffer"),
                                    contents: bytemuck::cast_slice(&slab_instances),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });

                            pass.set_pipeline(&slab_pipeline);
                            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            pass.set_vertex_buffer(1, instance_buffer.slice(..));
                            pass.set_index_buffer(
                                index_buffer.slice(..),
                                wgpu::IndexFormat::Uint16,
                            );
                            pass.draw_indexed(
                                0..QUAD_INDICES.len() as u32,
                                0,
                                0..slab_instances.len() as u32,
                            );
                        }

                        // Draw text
                        text_renderer
                            .render(&text_atlas, &viewport, &mut pass)
                            .unwrap();
                    }

                    queue.submit(std::iter::once(encoder.finish()));
                    frame.present();
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }
                _ => (),
            }
        })
        .unwrap();
}
