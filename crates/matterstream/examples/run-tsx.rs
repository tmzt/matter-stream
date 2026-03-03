use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

<<<<<<< HEAD
use matterstream::{MatterStream, OpsHeader, ops::RsiPointer, tier1::BankId};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{EventLoop, ControlFlow};
=======
use matterstream::{
    Compiler, MatterStream, OpsHeader, Parser, Validator, ops::RsiPointer, tier1::BankId,
};
use softbuffer::{Context, Surface};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
>>>>>>> 3b9a15a (Commit current work)
use winit::window::Window;

fn main() {
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
<<<<<<< HEAD
        eprintln!("Usage: cargo run --example run-tsx -- [--timeout <seconds>] <file.tsx>");
=======
        eprintln!(
            "Usage: cargo run -p matterstream --example run-tsx -- [--timeout <seconds>] <file.tsx>"
        );
>>>>>>> 3b9a15a (Commit current work)
        return;
    };

    let code = match fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
<<<<<<< HEAD
            eprintln!("Error reading file '{}': {}", file_path, e);
            return;
        }
    };
    let ops = match matterstream::Compiler::compile(&code) {
=======
            eprintln!("Error reading tsx file '{}': {}", file_path, e);
            return;
        }
    };

    let parsed = match matterstream::Parser::parse(&code) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("Error parsing tsx file: {}", e);
            return;
        }
    };

    let ops = match matterstream::Compiler::compile(&parsed) {
>>>>>>> 3b9a15a (Commit current work)
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("Error compiling tsx file: {}", e);
            return;
        }
    };

    // --- Winit and Softbuffer setup ---
    let event_loop = EventLoop::new().unwrap();
<<<<<<< HEAD
    let window = Arc::new(event_loop.create_window(Window::default_attributes().with_title("MatterStream Output")).unwrap());

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();
    
=======
    let window = Arc::new(
        event_loop
            .create_window(Window::default_attributes().with_title("MatterStream Output"))
            .unwrap(),
    );

    let context = Context::new(window.clone()).unwrap();
    let mut surface = Surface::new(&context, window.clone()).unwrap();

>>>>>>> 3b9a15a (Commit current work)
    // --- MatterStream setup ---
    let mut stream = MatterStream::new();
    let header = OpsHeader::new(vec![RsiPointer::new(1, BankId::Vec3 as u8, 0)], false);

<<<<<<< HEAD
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        
        // Request a redraw on every event to keep the animation loop running
        window.request_redraw();

        match event {
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                // --- MatterStream Execution ---
                smol::block_on(async {
                    if let Err(errors) = stream.execute(&header, &ops).await {
                        eprintln!("MatterStream execution failed with errors:");
                        for error in errors {
                            eprintln!("- {:?}", error);
                        }
                    }
                });

                // --- Rendering with Softbuffer ---
                let (width, height) = {
                    let size = window.inner_size();
                    (size.width, size.height)
                };
                surface
                    .resize(
                        NonZeroU32::new(width).unwrap(),
                        NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();

                let mut buffer = surface.buffer_mut().unwrap();
                buffer.fill(0xFF181818); // Fill with a dark gray background

                for draw in &stream.draws {
                    // Simple coordinate transformation from [-1, 1] to [0, width/height]
                    let x = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
                    let y = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

                    // Convert f32 RGBA to u32 0xRRGGBBAA
                    let r = (draw.color[0] * 255.0) as u32;
                    let g = (draw.color[1] * 255.0) as u32;
                    let b = (draw.color[2] * 255.0) as u32;
                    let a = (draw.color[3] * 255.0) as u32;
                    let color_u32 = (a << 24) | (r << 16) | (g << 8) | b;

                    // Draw a 10x10 colored square
                    for i in 0..10 {
                        for j in 0..10 {
                            let px = x + i;
                            let py = y + j;
                            if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                                let index = (py * width as i32 + px) as usize;
                                if index < buffer.len() {
                                    buffer[index] = color_u32;
=======
    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            // Request a redraw on every event to keep the animation loop running
            window.request_redraw();

            match event {
                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    // --- MatterStream Execution ---
                    smol::block_on(async {
                        if let Err(errors) = stream.execute(&header, &ops).await {
                            eprintln!("MatterStream execution failed with errors:");
                            for error in errors {
                                eprintln!("- {:?}", error);
                            }
                        }
                    });

                    // --- Rendering with Softbuffer ---
                    let (width, height) = {
                        let size = window.inner_size();
                        (size.width, size.height)
                    };
                    surface
                        .resize(
                            NonZeroU32::new(width).unwrap(),
                            NonZeroU32::new(height).unwrap(),
                        )
                        .unwrap();

                    let mut buffer = surface.buffer_mut().unwrap();
                    buffer.fill(0xFF181818); // Fill with a dark gray background

                    for draw in &stream.draws {
                        // Simple coordinate transformation from [-1, 1] to [0, width/height]
                        let x = ((draw.position[0] + 1.0) / 2.0 * width as f32) as i32;
                        let y = ((-draw.position[1] + 1.0) / 2.0 * height as f32) as i32;

                        // Convert f32 RGBA to u32 0xRRGGBBAA
                        let r = (draw.color[0] * 255.0) as u32;
                        let g = (draw.color[1] * 255.0) as u32;
                        let b = (draw.color[2] * 255.0) as u32;
                        let a = (draw.color[3] * 255.0) as u32;
                        let color_u32 = (a << 24) | (r << 16) | (g << 8) | b;

                        // Draw a 10x10 colored square
                        for i in 0..10 {
                            for j in 0..10 {
                                let px = x + i;
                                let py = y + j;
                                if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                                    let index = (py * width as i32 + px) as usize;
                                    if index < buffer.len() {
                                        buffer[index] = color_u32;
                                    }
>>>>>>> 3b9a15a (Commit current work)
                                }
                            }
                        }
                    }
<<<<<<< HEAD
                }
                
                buffer.present().unwrap();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                elwt.exit();
            }
            _ => (),
        }
    }).unwrap();
}
=======

                    buffer.present().unwrap();
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
>>>>>>> 3b9a15a (Commit current work)
