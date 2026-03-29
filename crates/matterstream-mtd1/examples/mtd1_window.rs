//! mtd1_window — Render the Tufte demo with system fonts via MSDF + wgpu.
//!
//! Usage:
//!   cargo run -p matterstream-mtd1 --example mtd1_window           # open window
//!   cargo run -p matterstream-mtd1 --example mtd1_window -- --png-out out.png  # render to PNG

use std::collections::HashMap;
use std::sync::Arc;

use matterstream_common::font::GpuFont;
use matterstream_common::pipeline::RenderFrame;
use matterstream_common::sdf::{SdfDrawCmd, DRAW_TYPE_MSDF_TEXT};
use matterstream_font::atlas::FontAtlasBuilder;
use matterstream_font::shaper::TextShaper;
use matterstream_mtd1::mtd1_format::{BankedStyle, Command32, Mtd1Document};
use matterstream_mtd1::mtd1_to_sdf::mtd1_to_sdf;
use matterstream_ui_gpu::GpuSdfRenderer;

/// Load a system font suitable for Tufte-style data display.
fn load_system_font() -> Vec<u8> {
    let candidates = [
        "/System/Library/Fonts/Supplemental/Georgia.ttf",
        "/Library/Fonts/Georgia.ttf",
        "/System/Library/Fonts/NewYork.ttf",
        "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
    ];
    for path in &candidates {
        if let Ok(data) = std::fs::read(path) {
            println!("Font: {}", path);
            return data;
        }
    }
    panic!("No system font found");
}

struct PreparedScene {
    draws: Vec<SdfDrawCmd>,
    char_buffer: Vec<u32>,
    glyph_table_u32s: Vec<u32>,
    atlas_rgba: Vec<u8>,
    atlas_width: u32,
    atlas_height: u32,
}

/// Check if a glyph ID corresponds to a non-printing character (space, etc.)
/// that should advance the cursor without emitting DRAW_GLYPH.
fn is_non_printing(shaper: &TextShaper, glyph_id: u16) -> bool {
    // Space glyph has no outline — skip it
    shaper.glyph_id_for_char(' ') == Some(glyph_id)
}

/// Emit shaped glyphs as DRAW_GLYPH instructions, skipping non-printing chars.
/// Non-printing glyphs (spaces) break the glyph run and emit SET_CURSOR to
/// jump past the gap, so the MSDF pipeline never sees them.
fn emit_shaped_run(
    doc: &mut Mtd1Document,
    shaper: &TextShaper,
    text: &str,
    scale: f32,
    cursor_x: &mut f32,
    cursor_y: f32,
) {
    let run = shaper.shape(text);
    let mut pending_advance: f32 = 0.0;

    for g in &run.glyphs {
        let adv_px = g.x_advance as f32 * scale;
        if is_non_printing(shaper, g.glyph_id) {
            // Accumulate advance for non-printing, don't emit DRAW_GLYPH
            pending_advance += adv_px;
        } else {
            // If we skipped non-printing chars, emit SET_CURSOR to jump past the gap
            if pending_advance > 0.0 {
                *cursor_x += pending_advance;
                doc.instructions.push(Command32::set_cursor(cursor_y as i16, *cursor_x as i16));
                pending_advance = 0.0;
            }
            let adv = (adv_px + 0.5) as u16;
            doc.instructions.push(Command32::draw_glyph(adv.max(1).min(4095), g.glyph_id));
            *cursor_x += adv_px;
        }
    }
    *cursor_x += pending_advance; // trailing space
}

fn build_scene() -> PreparedScene {
    let font_data = load_system_font();
    let shaper = TextShaper::new(font_data.clone()).expect("failed to create shaper");
    let font_size: f32 = 16.0;
    let px_range: f32 = 4.0;

    // Build MSDF atlas
    let mut atlas_builder = FontAtlasBuilder::new(font_data, 48, px_range as f64);
    atlas_builder.add_ascii();
    let atlas = atlas_builder.build().expect("atlas build failed");

    println!(
        "MSDF Atlas: {}x{}, {} glyphs, {}KB",
        atlas.width, atlas.height, atlas.glyphs.len(),
        atlas.pixel_data.len() / 1024
    );

    let mut glyph_id_to_table_index: HashMap<u16, u16> = HashMap::new();
    let mut standard_advances: HashMap<u16, f32> = HashMap::new();
    let mut glyph_table_u32s: Vec<u32> = Vec::new();
    for (i, entry) in atlas.glyphs.iter().enumerate() {
        glyph_id_to_table_index.insert(entry.glyph_id, i as u16);
        standard_advances.insert(entry.glyph_id, entry.advance_x);
        glyph_table_u32s.extend_from_slice(&entry.to_gpu_u32s());
    }

    // Shape and compile
    let upem = shaper.units_per_em();
    let scale = font_size / upem as f32;

    let paragraph = concat!(
        "The visual display of quantitative information demands that we give ",
        "the viewer the greatest number of ideas in the shortest time with ",
        "the least ink in the smallest space. Data graphics should draw ",
        "attention to the substance rather than to methodology, graphic ",
        "design, or technology of graphic production."
    );

    let mut doc = Mtd1Document::new();
    doc.styles.push(BankedStyle::new(0x1A1A2EFF, 0, 0, 0));
    doc.styles.push(BankedStyle::with_font(0xE8E6DFFF, 0, 0, 0, 1));
    doc.styles.push(BankedStyle::with_font(0xF5F0EBFF, 0, 0, 0, 0));
    doc.styles.push(BankedStyle::with_font(0xEDE8E0FF, 0, 0, 0, 0));
    doc.styles.push(BankedStyle::with_font(0xC75233FF, 2, 0, 1, 0));
    doc.styles.push(BankedStyle::with_font(0x333333FF, 0, 0, 0, 1));

    let max_width: f32 = 560.0;
    let origin_x: f32 = 20.0;
    let mut y: f32 = 24.0;

    doc.instructions.push(Command32::set_style(1));
    doc.instructions.push(Command32::set_cursor(y as i16, origin_x as i16));

    let space_w = shaper.shape(" ").total_advance as f32 * scale;

    let mut line_x: f32 = origin_x;
    let words: Vec<&str> = paragraph.split_whitespace().collect();
    for word in &words {
        let run = shaper.shape(word);
        let word_w = run.total_advance as f32 * scale;

        if line_x + word_w > origin_x + max_width && line_x > origin_x {
            y += font_size * 1.4;
            line_x = origin_x;
        }
        doc.instructions.push(Command32::set_cursor(y as i16, line_x as i16));
        emit_shaped_run(&mut doc, &shaper, word, scale, &mut line_x, y);
        line_x += space_w;
    }

    // Table
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
        if row_idx > 0 {
            let zebra_style = if row_idx % 2 == 0 { 2 } else { 3 };
            doc.instructions.push(Command32::set_style(zebra_style));
            doc.instructions.push(Command32::set_cursor(y as i16, origin_x as i16));
            doc.instructions.push(Command32::draw_shape(font_size as u16, 420));
        }
        doc.instructions.push(Command32::set_style(5));
        for (col, cell) in row.iter().enumerate() {
            let mut cx = col_x[col] as f32;
            doc.instructions.push(Command32::set_cursor(y as i16, col_x[col]));
            emit_shaped_run(&mut doc, &shaper, cell, scale, &mut cx, y);
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

    let sdf_frame = mtd1_to_sdf(&doc, &glyph_id_to_table_index, &standard_advances, font_size, px_range);

    println!(
        "Compiled: {} instructions → {} SdfDrawCmds, {} chars, {} MSDF text draws",
        doc.instructions.len(), sdf_frame.draws.len(), sdf_frame.char_buffer.len(),
        sdf_frame.draws.iter().filter(|d| d.draw_type() == DRAW_TYPE_MSDF_TEXT).count()
    );

    // RGB → RGBA for GPU
    let mut atlas_rgba = Vec::with_capacity((atlas.width * atlas.height * 4) as usize);
    for i in 0..(atlas.width * atlas.height) as usize {
        let src = i * 3;
        atlas_rgba.push(atlas.pixel_data.get(src).copied().unwrap_or(0));
        atlas_rgba.push(atlas.pixel_data.get(src + 1).copied().unwrap_or(0));
        atlas_rgba.push(atlas.pixel_data.get(src + 2).copied().unwrap_or(0));
        atlas_rgba.push(255);
    }

    PreparedScene {
        draws: sdf_frame.draws,
        char_buffer: sdf_frame.char_buffer,
        glyph_table_u32s,
        atlas_rgba,
        atlas_width: atlas.width,
        atlas_height: atlas.height,
    }
}

fn render_to_png(scene: &PreparedScene, path: &str, width: u32, height: u32) {
    let tex_format = wgpu::TextureFormat::Rgba8Unorm;
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        ..Default::default()
    })).expect("No GPU adapter");
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor::default()),
    ).expect("Failed to create device");

    let renderer = GpuSdfRenderer::new_with_msdf(&device, tex_format, scene.atlas_width, scene.atlas_height);
    renderer.upload_msdf_atlas(&queue, scene.atlas_width, scene.atlas_height, &scene.atlas_rgba);
    renderer.upload_glyph_table(&queue, &scene.glyph_table_u32s);
    renderer.upload_chars(&queue, &scene.char_buffer);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: tex_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());

    let frame = RenderFrame {
        draws: scene.draws.clone(),
        char_buffer: scene.char_buffer.clone(),
        anim_bank: vec![], texture_bank: vec![],
        font: GpuFont::NONE, glyph_bitmap: vec![],
        scalar_bank: [0.0; 16], int_bank: [0; 16],
        time_ms: 0.0, width, height, scale: 1.0,
    };
    renderer.render_frame(&device, &queue, &view, &frame);

    // Read back pixels
    let bpp = 4u32;
    let padded_row = (width * bpp + 255) & !255;
    let buf_size = (padded_row * height) as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"), size: buf_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_row), rows_per_image: None } },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().expect("buffer map failed");
    let data = slice.get_mapped_range();

    // Write PNG
    let file = std::fs::File::create(path).expect("failed to create PNG file");
    let mut enc = png::Encoder::new(file, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("PNG header");
    let mut img = Vec::with_capacity((width * height * bpp) as usize);
    for row in 0..height {
        let start = (row * padded_row) as usize;
        img.extend_from_slice(&data[start..start + (width * bpp) as usize]);
    }
    writer.write_image_data(&img).expect("PNG write");
    drop(data);
    readback.unmap();

    println!("Rendered {}x{} → {}", width, height, path);
}

fn run_window(scene: PreparedScene) {
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::Window;

    let event_loop = EventLoop::new().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop.create_window(
            Window::default_attributes()
                .with_title("mtd1 Tufte Demo [MSDF + HarfBuzz]")
                .with_inner_size(winit::dpi::LogicalSize::new(800, 500)),
        ).unwrap(),
    );

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

    let renderer = GpuSdfRenderer::new_with_msdf(&device, surface_format, scene.atlas_width, scene.atlas_height);
    renderer.upload_msdf_atlas(&queue, scene.atlas_width, scene.atlas_height, &scene.atlas_rgba);
    renderer.upload_glyph_table(&queue, &scene.glyph_table_u32s);
    renderer.upload_chars(&queue, &scene.char_buffer);

    let draws = scene.draws;
    let char_buffer = scene.char_buffer;

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
                let frame = RenderFrame {
                    draws: draws.clone(), char_buffer: char_buffer.clone(),
                    anim_bank: vec![], texture_bank: vec![],
                    font: GpuFont::NONE, glyph_bitmap: vec![],
                    scalar_bank: [0.0; 16], int_bank: [0; 16],
                    time_ms: 0.0, width: phys.width, height: phys.height,
                    scale: window.scale_factor() as f32,
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

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let scene = build_scene();

    // Check for --png-out flag
    let mut png_path = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--png-out" && i + 1 < args.len() {
            png_path = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }

    if let Some(path) = png_path {
        render_to_png(&scene, &path, 800, 500);
    } else {
        run_window(scene);
    }
}
