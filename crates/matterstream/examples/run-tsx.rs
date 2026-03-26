//! Run a TSX file through the msm1 compiler and VM pipeline.
//!
//! With --features compiler: compiles TSX → bytecode → executes → renders
//! With --features ui-softbuffer: renders SDF to window
//! Without features: prints SdfDrawCmd count
//!
//! Usage:
//!   cargo run -p matterstream --features compiler --example run-tsx -- <file.tsx>
//!   cargo run -p matterstream --features compiler,ui-softbuffer --example run-tsx -- [--timeout <s>] <file.tsx>

fn main() {
    #[cfg(not(feature = "compiler"))]
    {
        eprintln!("run-tsx requires --features compiler");
        return;
    }

    #[cfg(feature = "compiler")]
    run();
}

#[cfg(feature = "compiler")]
fn run() {
    use std::env;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    use matterstream_compiler::compile_to_asm;
    use matterstream::arena::TripleArena;
    use matterstream_vm::rpn::RpnVm;

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
                if !args[i].starts_with('-') {
                    file_path = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }

    if let Some(seconds) = timeout_s {
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(seconds));
            std::process::exit(0);
        });
    }

    let file_path = match file_path {
        Some(p) => p,
        None => {
            eprintln!("Usage: cargo run --features compiler --example run-tsx -- [--timeout <s>] <file.tsx>");
            return;
        }
    };

    let code = match fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading '{}': {}", file_path, e);
            return;
        }
    };

    let asm_output = match compile_to_asm(&code) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("Compile error: {}", e);
            return;
        }
    };

    println!("Compiled: {} bytes bytecode, {} strings", asm_output.bytecode.len(), asm_output.string_table.len());

    // Execute
    let mut arenas = TripleArena::new();
    let mut vm = RpnVm::new();
    vm.string_table = asm_output.string_table.clone();
    // Set security to INTERNAL for SystemCall access (SetOutputMode)
    vm.cr_bank[1] = matterstream_vm::rpn::SECURITY_INTERNAL as u32;

    if let Err(e) = vm.execute(&asm_output.bytecode, &mut arenas) {
        eprintln!("VM error: {:?}", e);
        return;
    }

    println!("Executed: {} SDF draw commands", vm.sdf_draws.len());
    for (i, cmd) in vm.sdf_draws.iter().enumerate() {
        println!("  [{i}] ty={} pos=({},{}) size=({},{}) color=({:.2},{:.2},{:.2},{:.2})",
            cmd.params[0] as u32, cmd.pos[0], cmd.pos[1], cmd.size[0], cmd.size[1],
            cmd.color[0], cmd.color[1], cmd.color[2], cmd.color[3]);
    }

    // Render with GPU SDF pipeline if available
    #[cfg(feature = "ui-gpu")]
    {
        use std::num::NonZeroU32;
        use std::sync::Arc;
        use matterstream_ui_gpu::GpuSdfRenderer;
        use winit::event::{Event, WindowEvent};
        use winit::event_loop::{EventLoop, ControlFlow};
        use winit::window::Window;

        let sdf_draws = vm.sdf_draws.clone();

        let event_loop = EventLoop::new().unwrap();
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(&format!("run-tsx [GPU]: {}", file_path))
                    .with_inner_size(winit::dpi::LogicalSize::new(1024, 120)),
            ).unwrap(),
        );

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })).expect("No suitable GPU adapter found");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
        )).expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats[0];
        let mut config = surface.get_default_config(&adapter, 400, 300).unwrap();
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&device, &config);

        let renderer = GpuSdfRenderer::new(&device, surface_format);

        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);
            match event {
                Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        config.width = size.width;
                        config.height = size.height;
                        surface.configure(&device, &config);

                        let frame = match surface.get_current_texture() {
                            wgpu::CurrentSurfaceTexture::Success(t)
                            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                            _ => { surface.configure(&device, &config); return; }
                        };
                        let view = frame.texture.create_view(&Default::default());
                        renderer.render(&device, &queue, &view, size.width, size.height, &sdf_draws);
                        frame.present();
                    }
                }
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    elwt.exit();
                }
                Event::WindowEvent { event: WindowEvent::Resized(_), .. } => {
                    window.request_redraw();
                }
                _ => (),
            }
        }).unwrap();

        return; // GPU path handles the event loop
    }

    // Fallback: render with CPU SDF if softbuffer available
    #[cfg(feature = "ui-softbuffer")]
    {
        use std::num::NonZeroU32;
        use std::sync::Arc;
        use matterstream_ui_soft::render_sdf;
        use softbuffer::{Context, Surface};
        use winit::event::{Event, WindowEvent};
        use winit::event_loop::{EventLoop, ControlFlow};
        use winit::window::Window;

        let sdf_draws = vm.sdf_draws.clone();

        let event_loop = EventLoop::new().unwrap();
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(&format!("run-tsx: {}", file_path))
                    .with_inner_size(winit::dpi::LogicalSize::new(1024, 120)),
            ).unwrap(),
        );

        let context = Context::new(window.clone()).unwrap();
        let mut surface = Surface::new(&context, window.clone()).unwrap();

        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);
            match event {
                Event::WindowEvent { event: WindowEvent::RedrawRequested, .. } => {
                    let phys = window.inner_size();
                    let pw = phys.width.max(1);
                    let ph = phys.height.max(1);
                    let scale = window.scale_factor() as u32;
                    let lw = pw / scale;
                    let lh = ph / scale;

                    surface.resize(NonZeroU32::new(pw).unwrap(), NonZeroU32::new(ph).unwrap()).unwrap();

                    let mut log_buf = vec![0x00181818u32; (lw * lh) as usize];
                    render_sdf(&sdf_draws, &mut log_buf, lw, lh);

                    let mut buffer = surface.buffer_mut().unwrap();
                    for py in 0..ph {
                        for px in 0..pw {
                            buffer[(py * pw + px) as usize] = log_buf[((py / scale) * lw + px / scale) as usize];
                        }
                    }
                    buffer.present().unwrap();
                }
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    elwt.exit();
                }
                _ => (),
            }
        }).unwrap();
    }

    #[cfg(not(feature = "ui-softbuffer"))]
    println!("No renderer — run with --features ui-softbuffer for window");
}
