//! Integration test: compile Tufte TSX → mtd1 → SdfDrawCmd → wgpu headless render → PNG.
//!
//! Renders the Tufte demo document to an offscreen texture using the GPU SDF
//! pipeline, reads pixels back, and verifies the output is non-trivial.

use matterstream_mtd1::pretext_rs::FontMetrics;
use matterstream_mtd1::tsx_to_mtd1::{TsxNode, compile_tsx};
use matterstream_mtd1::mtd1_to_sdf::{mtd1_to_sdf, generate_mini_font};

use matterstream_common::font::GpuFont;
use matterstream_common::pipeline::RenderFrame;

fn build_tufte_demo() -> Vec<TsxNode> {
    vec![TsxNode::TufteCard {
        x: 20,
        y: 10,
        width: 600,
        children: vec![
            TsxNode::Story {
                text: concat!(
                    "The visual display of quantitative information demands that we give ",
                    "the viewer the greatest number of ideas in the shortest time with ",
                    "the least ink in the smallest space."
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
                ],
                col_widths: vec![120, 100, 100, 120],
                zebra: true,
            },
            TsxNode::Path {
                segments: vec![
                    (2, 30), (4, 30), (3, 30), (7, 30), (5, 30),
                    (8, 30), (6, 30), (10, 30), (9, 30), (12, 30),
                ],
            },
        ],
    }]
}

#[test]
fn wgpu_headless_tufte_render() {
    // ── Phase 1: Compile TSX → mtd1 → SdfDrawCmd ────────────────────────
    let metrics = FontMetrics::monospace(8, 16);
    let tree = build_tufte_demo();
    let doc = compile_tsx(&tree, &metrics);
    let sdf_frame = mtd1_to_sdf(&doc);

    println!(
        "Compiled: {} mtd1 instructions → {} SdfDrawCmds, {} chars",
        doc.instructions.len(),
        sdf_frame.draws.len(),
        sdf_frame.char_buffer.len()
    );

    // ── Phase 2: Set up wgpu headless ───────────────────────────────────
    let width: u32 = 800;
    let height: u32 = 400;
    let tex_format = wgpu::TextureFormat::Rgba8Unorm;

    let instance = wgpu::Instance::default();

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: false,
        compatible_surface: None,
    }));

    let adapter = match adapter {
        Ok(a) => a,
        Err(_) => {
            eprintln!("SKIP: no wgpu adapter available (headless CI without GPU)");
            return;
        }
    };

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("mtd1_test"),
            ..Default::default()
        },
    ))
    .expect("failed to create wgpu device");

    // ── Phase 3: Create renderer and offscreen target ───────────────────
    let renderer = matterstream_ui_gpu::GpuSdfRenderer::new(&device, tex_format);

    let target_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: tex_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target_texture.create_view(&Default::default());

    // ── Phase 4: Build RenderFrame and render ───────────────────────────
    let (glyph_bitmap, font_params) = generate_mini_font();
    let font = GpuFont {
        glyph_w: font_params[0],
        glyph_h: font_params[1],
        first_cp: font_params[2],
        last_cp: font_params[3],
    };

    // Upload font and char data
    renderer.upload_font(&queue, &font, &glyph_bitmap);
    renderer.upload_chars(&queue, &sdf_frame.char_buffer);

    let frame = RenderFrame {
        draws: sdf_frame.draws.clone(),
        char_buffer: sdf_frame.char_buffer.clone(),
        anim_bank: vec![],
        texture_bank: vec![],
        font,
        glyph_bitmap,
        scalar_bank: [0.0; 16],
        int_bank: [0; 16],
        time_ms: 0.0,
        width,
        height,
        scale: 1.0,
    };

    renderer.render_frame(&device, &queue, &target_view, &frame);

    // ── Phase 5: Read back pixels ───────────────────────────────────────
    let bytes_per_pixel = 4u32;
    // wgpu requires rows aligned to 256 bytes
    let unpadded_row = width * bytes_per_pixel;
    let padded_row = (unpadded_row + 255) & !255;
    let buffer_size = (padded_row * height) as u64;

    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback_encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let buffer_slice = readback_buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().expect("buffer map failed");

    let data = buffer_slice.get_mapped_range();

    // ── Phase 6: Verify pixels ──────────────────────────────────────────
    // Count non-background pixels (background is ~(25, 25, 30) dark theme)
    let mut non_bg_pixels = 0u64;
    let mut total_pixels = 0u64;
    for row in 0..height {
        let row_start = (row * padded_row) as usize;
        for col in 0..width {
            let px = row_start + (col * bytes_per_pixel) as usize;
            let r = data[px];
            let g = data[px + 1];
            let b = data[px + 2];
            total_pixels += 1;
            // Background is approximately (25, 25, 30) — anything significantly different is content
            if r > 40 || g > 40 || b > 45 {
                non_bg_pixels += 1;
            }
        }
    }

    let content_ratio = non_bg_pixels as f64 / total_pixels as f64;
    println!(
        "Render result: {}x{}, {} non-bg pixels ({:.2}% content)",
        width,
        height,
        non_bg_pixels,
        content_ratio * 100.0
    );

    // Should have some rendered content (text, shapes, zebra stripes)
    assert!(
        non_bg_pixels > 100,
        "Expected rendered content but got only {} non-bg pixels",
        non_bg_pixels
    );

    // ── Phase 7: Save PNG for visual inspection ─────────────────────────
    let png_path = std::env::temp_dir().join("tufte_mtd1_wgpu.png");
    let file = std::fs::File::create(&png_path).expect("create png file");
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");

    // Copy rows without padding
    let mut img_data = Vec::with_capacity((width * height * bytes_per_pixel) as usize);
    for row in 0..height {
        let row_start = (row * padded_row) as usize;
        let row_end = row_start + (width * bytes_per_pixel) as usize;
        img_data.extend_from_slice(&data[row_start..row_end]);
    }
    writer.write_image_data(&img_data).expect("write png data");

    drop(data);
    readback_buffer.unmap();

    println!("PNG saved to: {}", png_path.display());
    println!("PASS: wgpu headless Tufte render succeeded");
}
