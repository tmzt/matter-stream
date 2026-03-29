//! letter_grid — Render Latin characters in a simple grid to debug MSDF rendering.
//!
//! Usage:
//!   cargo run -p matterstream-mtd1 --example letter_grid -- --png-out out.png

use std::collections::HashMap;
use std::sync::Arc;

use matterstream_common::font::GpuFont;
use matterstream_common::pipeline::RenderFrame;
use matterstream_font::atlas::FontAtlasBuilder;
use matterstream_font::shaper::TextShaper;
use matterstream_mtd1::mtd1_format::{BankedStyle, Command32, Mtd1Document};
use matterstream_mtd1::mtd1_to_sdf::mtd1_to_sdf;
use matterstream_ui_gpu::GpuSdfRenderer;

fn load_system_font() -> Vec<u8> {
    let candidates = [
        "/System/Library/Fonts/Supplemental/Georgia.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
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

fn main() {
    let font_data = load_system_font();
    let shaper = TextShaper::new(font_data.clone()).unwrap();
    let font_size: f32 = 48.0;
    let px_range: f32 = 4.0;
    let cell_w: f32 = 52.0;
    let cell_h: f32 = 60.0;
    let origin_x: f32 = 30.0;
    let origin_y: f32 = 30.0;

    // Build MSDF atlas
    let mut builder = FontAtlasBuilder::new(font_data, 128, px_range as f64);
    builder.add_ascii();
    let atlas = builder.build().expect("atlas build failed");
    println!("Atlas: {}x{}, {} glyphs", atlas.width, atlas.height, atlas.glyphs.len());

    let mut gid_to_idx: HashMap<u16, u16> = HashMap::new();
    let mut std_advances: HashMap<u16, f32> = HashMap::new();
    let mut glyph_table_u32s: Vec<u32> = Vec::new();
    for (i, e) in atlas.glyphs.iter().enumerate() {
        gid_to_idx.insert(e.glyph_id, i as u16);
        std_advances.insert(e.glyph_id, e.advance_x);
        glyph_table_u32s.extend_from_slice(&e.to_gpu_u32s());
    }

    let upem = shaper.units_per_em();
    let scale = font_size / upem as f32;

    // Simple grid: each character gets its own SET_CURSOR + DRAW_GLYPH
    let rows: &[&str] = &[
        "ABCDEFGHIJKLM",
        "NOPQRSTUVWXYZ",
        "abcdefghijklm",
        "nopqrstuvwxyz",
        "0123456789",
        ".,:;!?+-=$%",
    ];

    let mut doc = Mtd1Document::new();
    doc.styles.push(BankedStyle::with_font(0x111111FF, 0, 0, 0, 1));

    doc.instructions.push(Command32::set_style(0));

    for (row_idx, row) in rows.iter().enumerate() {
        let y = origin_y + row_idx as f32 * cell_h;
        for (col_idx, ch) in row.chars().enumerate() {
            let x = origin_x + col_idx as f32 * cell_w;
            doc.instructions.push(Command32::set_cursor(y as i16, x as i16));

            let s = ch.to_string();
            let run = shaper.shape(&s);
            for g in &run.glyphs {
                let adv = (g.x_advance as f32 * scale + 0.5) as u16;
                doc.instructions.push(Command32::draw_glyph(adv.max(1).min(4095), g.glyph_id));
            }
        }
    }

    let sdf_frame = mtd1_to_sdf(&doc, &gid_to_idx, &std_advances, font_size, px_range);
    println!("Draws: {}, chars: {}", sdf_frame.draws.len(), sdf_frame.char_buffer.len());

    // RGB → RGBA
    let mut atlas_rgba = Vec::with_capacity((atlas.width * atlas.height * 4) as usize);
    for i in 0..(atlas.width * atlas.height) as usize {
        let s = i * 3;
        atlas_rgba.push(atlas.pixel_data.get(s).copied().unwrap_or(0));
        atlas_rgba.push(atlas.pixel_data.get(s + 1).copied().unwrap_or(0));
        atlas_rgba.push(atlas.pixel_data.get(s + 2).copied().unwrap_or(0));
        atlas_rgba.push(255);
    }

    // Check args
    let args: Vec<String> = std::env::args().collect();
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

    let width = 750u32;
    let height = 420u32;

    if let Some(path) = png_path {
        // Offscreen render
        let tex_format = wgpu::TextureFormat::Rgba8Unorm;
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None, ..Default::default()
        })).expect("No GPU adapter");
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default()),
        ).unwrap();

        let renderer = GpuSdfRenderer::new_with_msdf(&device, tex_format, atlas.width, atlas.height);
        renderer.upload_msdf_atlas(&queue, atlas.width, atlas.height, &atlas_rgba);
        renderer.upload_glyph_table(&queue, &glyph_table_u32s);
        renderer.upload_chars(&queue, &sdf_frame.char_buffer);

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
            draws: sdf_frame.draws, char_buffer: sdf_frame.char_buffer,
            anim_bank: vec![], texture_bank: vec![],
            font: GpuFont::NONE, glyph_bitmap: vec![],
            scalar_bank: [0.0; 16], int_bank: [0; 16],
            time_ms: 0.0, width, height, scale: 1.0,
        };
        renderer.render_frame(&device, &queue, &view, &frame);

        // Readback + PNG
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

        let file = std::fs::File::create(&path).unwrap();
        let mut png_enc = png::Encoder::new(file, width, height);
        png_enc.set_color(png::ColorType::Rgba);
        png_enc.set_depth(png::BitDepth::Eight);
        let mut w = png_enc.write_header().unwrap();
        let mut img = Vec::with_capacity((width * height * bpp) as usize);
        for row in 0..height {
            let rs = (row * padded_row) as usize;
            img.extend_from_slice(&data[rs..rs + (width * bpp) as usize]);
        }
        w.write_image_data(&img).unwrap();
        drop(data);
        readback.unmap();
        println!("Rendered {}x{} → {}", width, height, path);
    } else {
        // Window mode
        use winit::event::{Event, WindowEvent};
        use winit::event_loop::{ControlFlow, EventLoop};
        use winit::window::Window;

        let event_loop = EventLoop::new().unwrap();
        #[allow(deprecated)]
        let window = Arc::new(event_loop.create_window(
            Window::default_attributes()
                .with_title("Letter Grid [MSDF]")
                .with_inner_size(winit::dpi::LogicalSize::new(width, height)),
        ).unwrap());

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface), ..Default::default()
        })).expect("No GPU adapter");
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default()),
        ).unwrap();

        let caps = surface.get_capabilities(&adapter);
        let fmt = caps.formats.iter().find(|f| !f.is_srgb()).copied().unwrap_or(caps.formats[0]);
        let sz = window.inner_size();
        let mut config = surface.get_default_config(&adapter, sz.width.max(1), sz.height.max(1)).unwrap();
        config.format = fmt;
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&device, &config);

        let renderer = GpuSdfRenderer::new_with_msdf(&device, fmt, atlas.width, atlas.height);
        renderer.upload_msdf_atlas(&queue, atlas.width, atlas.height, &atlas_rgba);
        renderer.upload_glyph_table(&queue, &glyph_table_u32s);
        renderer.upload_chars(&queue, &sdf_frame.char_buffer);

        let draws = sdf_frame.draws;
        let char_buf = sdf_frame.char_buffer;

        #[allow(deprecated)]
        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);
            match event {
                Event::AboutToWait => { window.request_redraw(); }
                Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
                    let p = window.inner_size();
                    if p.width == 0 || p.height == 0 { return; }
                    config.width = p.width; config.height = p.height;
                    surface.configure(&device, &config);
                    let ft = match surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(t)
                        | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                        _ => return,
                    };
                    let v = ft.texture.create_view(&Default::default());
                    let frame = RenderFrame {
                        draws: draws.clone(), char_buffer: char_buf.clone(),
                        anim_bank: vec![], texture_bank: vec![],
                        font: GpuFont::NONE, glyph_bitmap: vec![],
                        scalar_bank: [0.0; 16], int_bank: [0; 16],
                        time_ms: 0.0, width: p.width, height: p.height,
                        scale: window.scale_factor() as f32,
                    };
                    renderer.render_frame(&device, &queue, &v, &frame);
                    ft.present();
                }
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => { elwt.exit(); }
                Event::WindowEvent { event: WindowEvent::Resized(_), .. } => { window.request_redraw(); }
                _ => (),
            }
        }).unwrap();
    }
}
