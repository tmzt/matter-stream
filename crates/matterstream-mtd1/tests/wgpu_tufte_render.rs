//! Integration test: compile Tufte TSX → mtd1 → SdfDrawCmd → wgpu headless → PNG.
//!
//! Uses the MSDF text path (default) with system fonts.

use std::collections::HashMap;

use matterstream_common::font::GpuFont;
use matterstream_common::pipeline::RenderFrame;
use matterstream_font::atlas::FontAtlasBuilder;
use matterstream_font::shaper::TextShaper;
use matterstream_mtd1::mtd1_format::{BankedStyle, Command32, Mtd1Document};
use matterstream_mtd1::mtd1_to_sdf::mtd1_to_sdf;

fn load_system_font() -> Option<Vec<u8>> {
    let paths = [
        "/Users/tmeade/src/Projects/CasualEffects/TerminalVelocity/worktrees/game-terminalvelocity1/game-connect4/assets/fonts/Inter-Regular.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/Library/Fonts/Arial.ttf",
    ];
    for p in &paths {
        if let Ok(d) = std::fs::read(p) { return Some(d); }
    }
    None
}

#[test]
fn wgpu_headless_tufte_render() {
    let font_data = match load_system_font() {
        Some(d) => d,
        None => { eprintln!("SKIP: no system font"); return; }
    };

    let shaper = TextShaper::new(font_data.clone()).unwrap();
    let font_size: f32 = 16.0;
    let px_range: f32 = 4.0;
    let scale = font_size / shaper.units_per_em() as f32;

    // Build MSDF atlas
    let mut builder = FontAtlasBuilder::new(font_data, 48, px_range as f64);
    builder.add_ascii();
    let atlas = builder.build().expect("atlas build");

    let mut gid_to_idx: HashMap<u16, u16> = HashMap::new();
    let mut std_advances: HashMap<u16, f32> = HashMap::new();
    let mut glyph_table_u32s: Vec<u32> = Vec::new();
    for (i, e) in atlas.glyphs.iter().enumerate() {
        gid_to_idx.insert(e.glyph_id, i as u16);
        std_advances.insert(e.glyph_id, e.advance_x);
        glyph_table_u32s.extend_from_slice(&e.to_gpu_u32s());
    }

    // Build document
    let mut doc = Mtd1Document::new();
    doc.styles.push(BankedStyle::with_font(0xE8E6DFFF, 0, 0, 0, 1));

    doc.instructions.push(Command32::set_style(0));
    doc.instructions.push(Command32::set_cursor(20, 20));

    let text = "The visual display of quantitative information.";
    let run = shaper.shape(text);
    for g in &run.glyphs {
        let adv = (g.x_advance as f32 * scale + 0.5) as u16;
        doc.instructions.push(Command32::draw_glyph(adv.max(1).min(4095), g.glyph_id));
    }

    let sdf_frame = mtd1_to_sdf(&doc, &gid_to_idx, &std_advances, font_size, px_range);

    println!("Draws: {}, chars: {}", sdf_frame.draws.len(), sdf_frame.char_buffer.len());

    // wgpu headless
    let width: u32 = 800;
    let height: u32 = 200;
    let tex_format = wgpu::TextureFormat::Rgba8Unorm;

    let instance = wgpu::Instance::default();
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None, ..Default::default()
    })) {
        Ok(a) => a,
        Err(_) => { eprintln!("SKIP: no GPU"); return; }
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();

    let renderer = matterstream_ui_gpu::GpuSdfRenderer::new_with_msdf(&device, tex_format, atlas.width, atlas.height);

    // Upload atlas
    let mut rgba = Vec::with_capacity((atlas.width * atlas.height * 4) as usize);
    for i in 0..(atlas.width * atlas.height) as usize {
        let s = i * 3;
        rgba.push(atlas.pixel_data.get(s).copied().unwrap_or(0));
        rgba.push(atlas.pixel_data.get(s+1).copied().unwrap_or(0));
        rgba.push(atlas.pixel_data.get(s+2).copied().unwrap_or(0));
        rgba.push(255);
    }
    renderer.upload_msdf_atlas(&queue, atlas.width, atlas.height, &rgba);
    renderer.upload_glyph_table(&queue, &glyph_table_u32s);
    renderer.upload_chars(&queue, &sdf_frame.char_buffer);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"), size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: tex_format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());

    let frame = RenderFrame {
        draws: sdf_frame.draws, char_buffer: sdf_frame.char_buffer,
        anim_bank: vec![], texture_bank: vec![], font: GpuFont::NONE,
        glyph_bitmap: vec![], scalar_bank: [0.0; 16], int_bank: [0; 16],
        time_ms: 0.0, width, height, scale: 1.0,
    };
    renderer.render_frame(&device, &queue, &view, &frame);

    // Readback
    let bpp = 4u32;
    let padded_row = (width * bpp + 255) & !255;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb"), size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_row), rows_per_image: None } },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(enc.finish()));
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| { tx.send(r).unwrap(); });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range();

    let mut content = 0u64;
    for row in 0..height {
        let rs = (row * padded_row) as usize;
        for col in 0..width {
            let px = rs + (col * bpp) as usize;
            if data[px] > 40 || data[px+1] > 40 || data[px+2] > 45 { content += 1; }
        }
    }
    println!("Content: {} px ({:.1}%)", content, content as f64 / (width * height) as f64 * 100.0);
    assert!(content > 50, "should have rendered MSDF text");

    // Save PNG
    let png_path = "/Users/tmeade/src/common-data/tufte_mtd1_wgpu.png";
    let file = std::fs::File::create(&png_path).unwrap();
    let mut enc = png::Encoder::new(file, width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().unwrap();
    let mut img = Vec::with_capacity((width * height * bpp) as usize);
    for row in 0..height {
        let rs = (row * padded_row) as usize;
        img.extend_from_slice(&data[rs..rs + (width * bpp) as usize]);
    }
    w.write_image_data(&img).unwrap();
    drop(data);
    readback.unmap();

    println!("PNG: {}", png_path);
    println!("PASS: MSDF text rendering via wgpu");
}
