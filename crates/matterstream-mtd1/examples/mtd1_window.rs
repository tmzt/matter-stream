//! mtd1_window — Render the Tufte demo to a macOS window via wgpu.
//!
//! Usage:
//!   cargo run -p matterstream-mtd1 --example mtd1_window

use std::sync::Arc;

use matterstream_common::font::GpuFont;
use matterstream_common::pipeline::RenderFrame;
use matterstream_mtd1::mtd1_to_sdf::{generate_mini_font, mtd1_to_sdf};
use matterstream_mtd1::pretext_rs::FontMetrics;
use matterstream_mtd1::tsx_to_mtd1::{compile_tsx, TsxNode};
use matterstream_ui_gpu::GpuSdfRenderer;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

fn build_tufte_demo() -> Vec<TsxNode> {
    vec![TsxNode::TufteCard {
        x: 20,
        y: 20,
        width: 560,
        children: vec![
            TsxNode::Story {
                text: concat!(
                    "The visual display of quantitative information demands that we give ",
                    "the viewer the greatest number of ideas in the shortest time with ",
                    "the least ink in the smallest space. Data graphics should draw ",
                    "attention to the substance rather than to methodology, graphic ",
                    "design, or technology of graphic production."
                )
                .into(),
                token: Some((1, 1001)),
            },
            TsxNode::Spreadsheet {
                headers: vec![
                    "Quarter".into(),
                    "Revenue".into(),
                    "Growth".into(),
                    "Margin".into(),
                ],
                rows: vec![
                    vec!["Q1 2024".into(), "$12.4M".into(), "+8.2%".into(), "34.1%".into()],
                    vec!["Q2 2024".into(), "$13.1M".into(), "+5.6%".into(), "35.8%".into()],
                    vec!["Q3 2024".into(), "$14.8M".into(), "+13.0%".into(), "36.2%".into()],
                    vec!["Q4 2024".into(), "$15.2M".into(), "+2.7%".into(), "37.0%".into()],
                    vec!["Q1 2025".into(), "$16.9M".into(), "+11.2%".into(), "38.4%".into()],
                    vec!["Q2 2025".into(), "$18.3M".into(), "+8.3%".into(), "39.1%".into()],
                ],
                col_widths: vec![120, 100, 100, 120],
                zebra: true,
            },
            TsxNode::Path {
                segments: vec![
                    (2, 30),
                    (4, 30),
                    (3, 30),
                    (7, 30),
                    (5, 30),
                    (8, 30),
                    (6, 30),
                    (10, 30),
                    (9, 30),
                    (12, 30),
                    (11, 30),
                    (14, 30),
                ],
            },
        ],
    }]
}

fn main() {
    // ── Compile TSX → mtd1 → SdfDrawCmd ─────────────────────────────────
    let metrics = FontMetrics::monospace(8, 16);
    let tree = build_tufte_demo();
    let doc = compile_tsx(&tree, &metrics);
    let sdf_frame = mtd1_to_sdf(&doc);
    let (glyph_bitmap, font_params) = generate_mini_font();
    let font = GpuFont {
        glyph_w: font_params[0],
        glyph_h: font_params[1],
        first_cp: font_params[2],
        last_cp: font_params[3],
    };

    println!(
        "Compiled: {} mtd1 instructions → {} SdfDrawCmds, {} chars",
        doc.instructions.len(),
        sdf_frame.draws.len(),
        sdf_frame.char_buffer.len()
    );

    // ── Create window ───────────────────────────────────────────────────
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("mtd1 Tufte Demo [wgpu]")
                    .with_inner_size(winit::dpi::LogicalSize::new(800, 500)),
            )
            .unwrap(),
    );

    // ── Set up wgpu ─────────────────────────────────────────────────────
    let instance = wgpu::Instance::default();
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    }))
    .expect("No suitable GPU adapter found");

    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor::default()),
    )
    .expect("Failed to create device");

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps
        .formats
        .iter()
        .find(|f| !f.is_srgb())
        .copied()
        .unwrap_or(surface_caps.formats[0]);

    let init_size = window.inner_size();
    let mut config = surface
        .get_default_config(&adapter, init_size.width.max(1), init_size.height.max(1))
        .unwrap();
    config.format = surface_format;
    config.present_mode = wgpu::PresentMode::Fifo;
    surface.configure(&device, &config);

    // ── Create renderer and upload font ─────────────────────────────────
    let renderer = GpuSdfRenderer::new(&device, surface_format);
    renderer.upload_font(&queue, &font, &glyph_bitmap);
    renderer.upload_chars(&queue, &sdf_frame.char_buffer);

    let draws = sdf_frame.draws.clone();
    let char_buffer = sdf_frame.char_buffer.clone();

    // ── Event loop ──────────────────────────────────────────────────────
    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);
            match event {
                Event::AboutToWait => {
                    window.request_redraw();
                }
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    let phys = window.inner_size();
                    if phys.width == 0 || phys.height == 0 {
                        return;
                    }

                    config.width = phys.width;
                    config.height = phys.height;
                    surface.configure(&device, &config);

                    let frame_tex = match surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(t)
                        | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                        _ => return,
                    };
                    let view = frame_tex.texture.create_view(&Default::default());
                    let scale = window.scale_factor() as f32;

                    let frame = RenderFrame {
                        draws: draws.clone(),
                        char_buffer: char_buffer.clone(),
                        anim_bank: vec![],
                        texture_bank: vec![],
                        font,
                        glyph_bitmap: glyph_bitmap.clone(),
                        scalar_bank: [0.0; 16],
                        int_bank: [0; 16],
                        time_ms: 0.0,
                        width: phys.width,
                        height: phys.height,
                        scale,
                    };

                    renderer.render_frame(&device, &queue, &view, &frame);
                    frame_tex.present();
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => {
                    window.request_redraw();
                }
                _ => (),
            }
        })
        .unwrap();
}
