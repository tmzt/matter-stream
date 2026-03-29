//! mtd1_window — Render the Tufte demo with system fonts via MSDF + wgpu.
//!
//! Uses HarfBuzz (rustybuzz) for text shaping and msdfgen for glyph atlas
//! generation, rendering through the GPU SDF pipeline.
//!
//! Usage:
//!   cargo run -p matterstream-mtd1 --example mtd1_window

use std::collections::HashMap;
use std::sync::Arc;

use matterstream_common::font::GpuFont;
use matterstream_common::pipeline::RenderFrame;
use matterstream_common::sdf::DRAW_TYPE_MSDF_TEXT;
use matterstream_font::atlas::FontAtlasBuilder;
use matterstream_font::shaper::TextShaper;
use matterstream_mtd1::mtd1_format::{BankedStyle, Command32, Mtd1Document};
use matterstream_mtd1::mtd1_to_sdf::mtd1_to_sdf_msdf;
use matterstream_ui_gpu::GpuSdfRenderer;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

/// Load a system font suitable for Tufte-style data display.
fn load_system_font() -> Vec<u8> {
    let candidates = [
        // Tufte-appropriate serif/body fonts
        "/System/Library/Fonts/Supplemental/Georgia.ttf",
        "/Library/Fonts/Georgia.ttf",
        "/System/Library/Fonts/NewYork.ttf",
        "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
        // Clean sans fallbacks
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
        "/System/Library/Fonts/SFNSText.ttf",
    ];
    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            println!("Font: {}", path);
            return data;
        }
    }
    panic!("No system font found");
}

fn main() {
    let font_data = load_system_font();
    let shaper = TextShaper::new(font_data.clone()).expect("failed to create shaper");
    let font_size: f32 = 16.0;
    let px_range: f32 = 4.0;

    // ── Build MSDF atlas ────────────────────────────────────────────────
    let mut atlas_builder = FontAtlasBuilder::new(font_data, 48, px_range as f64);
    atlas_builder.add_ascii();
    let atlas = atlas_builder.build().expect("atlas build failed");

    println!(
        "MSDF Atlas: {}x{}, {} glyphs, {}KB",
        atlas.width, atlas.height, atlas.glyphs.len(),
        atlas.pixel_data.len() / 1024
    );

    // Build glyph_id → table_index map and GPU glyph table
    let mut glyph_id_to_table_index: HashMap<u16, u16> = HashMap::new();
    let mut glyph_table_u32s: Vec<u32> = Vec::new();
    for (i, entry) in atlas.glyphs.iter().enumerate() {
        glyph_id_to_table_index.insert(entry.glyph_id, i as u16);
        let packed = entry.to_gpu_u32s();
        glyph_table_u32s.extend_from_slice(&packed);
    }

    // ── Shape and compile text ──────────────────────────────────────────
    let upem = shaper.units_per_em();
    let scale = font_size / upem as f32;

    let paragraph = concat!(
        "The visual display of quantitative information demands that we give ",
        "the viewer the greatest number of ideas in the shortest time with ",
        "the least ink in the smallest space. Data graphics should draw ",
        "attention to the substance rather than to methodology, graphic ",
        "design, or technology of graphic production."
    );

    // Build mtd1 document with shaped text
    let mut doc = Mtd1Document::new();
    // Style 0: dark bg
    doc.styles.push(BankedStyle::new(0x1A1A2EFF, 0, 0, 0));
    // Style 1: text with MSDF font (font_index=1)
    doc.styles.push(BankedStyle::with_font(0xE8E6DFFF, 0, 0, 0, 1));
    // Style 2: zebra even (font_index=1)
    doc.styles.push(BankedStyle::with_font(0xF5F0EBFF, 0, 0, 0, 0));
    // Style 3: zebra odd
    doc.styles.push(BankedStyle::with_font(0xEDE8E0FF, 0, 0, 0, 0));
    // Style 4: sparkline
    doc.styles.push(BankedStyle::with_font(0xC75233FF, 2, 0, 1, 0));
    // Style 5: table text with MSDF font
    doc.styles.push(BankedStyle::with_font(0x333333FF, 0, 0, 0, 1));

    // Layout paragraph with line wrapping using HarfBuzz
    let max_width: f32 = 560.0;
    let origin_x: f32 = 20.0;
    let mut y: f32 = 24.0;

    doc.instructions.push(Command32::set_style(1));

    // Word-wrap with shaping
    let words: Vec<&str> = paragraph.split_whitespace().collect();
    let mut line_x: f32 = origin_x;
    doc.instructions.push(Command32::set_cursor(y as i16, origin_x as i16));

    for word in &words {
        let run = shaper.shape(word);
        let word_width = run.total_advance as f32 * scale;
        let space_run = shaper.shape(" ");
        let space_width = space_run.total_advance as f32 * scale;

        if line_x + word_width > origin_x + max_width && line_x > origin_x {
            y += font_size * 1.4;
            line_x = origin_x;
            doc.instructions.push(Command32::set_cursor(y as i16, origin_x as i16));
        }

        for glyph in &run.glyphs {
            let advance_px = (glyph.x_advance as f32 * scale) as u16;
            doc.instructions.push(Command32::draw_glyph(
                advance_px.min(4095),
                glyph.glyph_id,
            ));
        }
        line_x += word_width + space_width;
    }

    // Table data
    y += font_size * 2.0;
    let table_data = [
        ["Quarter", "Revenue", "Growth", "Margin"],
        ["Q1 2024", "$12.4M", "+8.2%", "34.1%"],
        ["Q2 2024", "$13.1M", "+5.6%", "35.8%"],
        ["Q3 2024", "$14.8M", "+13.0%", "36.2%"],
        ["Q4 2024", "$15.2M", "+2.7%", "37.0%"],
    ];
    let col_x = [20i16, 140, 240, 340];

    for (row_idx, row) in table_data.iter().enumerate() {
        // Zebra background for data rows
        if row_idx > 0 {
            let zebra_style = if row_idx % 2 == 0 { 2 } else { 3 };
            doc.instructions.push(Command32::set_style(zebra_style));
            doc.instructions.push(Command32::set_cursor(y as i16, origin_x as i16));
            doc.instructions.push(Command32::draw_shape(font_size as u16, 420));
        }

        doc.instructions.push(Command32::set_style(5));
        for (col, cell) in row.iter().enumerate() {
            doc.instructions.push(Command32::set_cursor(y as i16, col_x[col]));
            let run = shaper.shape(cell);
            for glyph in &run.glyphs {
                let advance_px = (glyph.x_advance as f32 * scale) as u16;
                doc.instructions.push(Command32::draw_glyph(advance_px.min(4095), glyph.glyph_id));
            }
        }
        y += font_size * 1.3;
    }

    // Sparkline
    y += font_size * 0.5;
    doc.instructions.push(Command32::set_style(4));
    doc.instructions.push(Command32::set_cursor(y as i16, origin_x as i16));
    for &(h, w) in &[(2, 30), (4, 30), (3, 30), (7, 30), (5, 30), (8, 30), (6, 30), (10, 30), (9, 30), (12, 30)] {
        doc.instructions.push(Command32::draw_shape(h, w));
    }

    // Convert to SDF draws
    let sdf_frame = mtd1_to_sdf_msdf(&doc, &glyph_id_to_table_index, font_size, px_range);

    println!(
        "Compiled: {} instructions → {} SdfDrawCmds, {} chars",
        doc.instructions.len(), sdf_frame.draws.len(), sdf_frame.char_buffer.len()
    );

    let msdf_count = sdf_frame.draws.iter().filter(|d| d.draw_type() == DRAW_TYPE_MSDF_TEXT).count();
    println!("MSDF text draws: {}", msdf_count);

    // ── Create window ───────────────────────────────────────────────────
    let event_loop = EventLoop::new().unwrap();
    let window = Arc::new(
        event_loop.create_window(
            Window::default_attributes()
                .with_title("mtd1 Tufte Demo [MSDF + HarfBuzz]")
                .with_inner_size(winit::dpi::LogicalSize::new(800, 500)),
        ).unwrap(),
    );

    // ── Set up wgpu ─────────────────────────────────────────────────────
    let instance = wgpu::Instance::default();
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&surface),
        ..Default::default()
    })).expect("No GPU adapter");
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor::default()),
    ).expect("Failed to create device");

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps.formats.iter()
        .find(|f| !f.is_srgb()).copied()
        .unwrap_or(surface_caps.formats[0]);
    let init_size = window.inner_size();
    let mut config = surface.get_default_config(&adapter, init_size.width.max(1), init_size.height.max(1)).unwrap();
    config.format = surface_format;
    config.present_mode = wgpu::PresentMode::Fifo;
    surface.configure(&device, &config);

    // ── Create renderer with MSDF atlas dimensions ────────────────────
    let renderer = GpuSdfRenderer::new_with_msdf(&device, surface_format, atlas.width, atlas.height);

    // Upload MSDF atlas (RGB → RGBA conversion for Rgba8Unorm)
    let mut rgba_data = Vec::with_capacity((atlas.width * atlas.height * 4) as usize);
    for i in 0..(atlas.width * atlas.height) as usize {
        let src = i * 3;
        rgba_data.push(atlas.pixel_data.get(src).copied().unwrap_or(0));
        rgba_data.push(atlas.pixel_data.get(src + 1).copied().unwrap_or(0));
        rgba_data.push(atlas.pixel_data.get(src + 2).copied().unwrap_or(0));
        rgba_data.push(255);
    }
    renderer.upload_msdf_atlas(&queue, atlas.width, atlas.height, &rgba_data);
    renderer.upload_glyph_table(&queue, &glyph_table_u32s);
    renderer.upload_chars(&queue, &sdf_frame.char_buffer);

    let draws = sdf_frame.draws.clone();
    let char_buffer = sdf_frame.char_buffer.clone();

    // ── Event loop ──────────────────────────────────────────────────────
    #[allow(deprecated)]
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        match event {
            Event::AboutToWait => { window.request_redraw(); }
            Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
                let phys = window.inner_size();
                if phys.width == 0 || phys.height == 0 { return; }
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
                    font: GpuFont::NONE,
                    glyph_bitmap: vec![],
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
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => { elwt.exit(); }
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } => { window.request_redraw(); }
            _ => (),
        }
    }).unwrap();
}
