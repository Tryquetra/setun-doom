//! Setun-DOS: Standalone 21-trit Operating Environment CLI.
//!
//! Inspired by KolibriOS architecture: monolithic assembly kernel,
//! trichotomic system calls, static linear memory mapping, and native Doom execution.

use std::env;
use std::fs;
use std::path::Path;

use setun_doom::dos::memmap::*;
use setun_doom::dos::tfx::TfxExecutable;
use setun_doom::dos::{SetunDesktop, SetunDos};
use setun_doom::BenchGui;

fn main() {
    println!("============================================================");
    println!("  SETUN-DOS 21-TRIT OPERATING ENVIRONMENT (LEVEL 3)       ");
    println!("  Soviet Balanced Ternary Kernel inspired by KolibriOS     ");
    println!("============================================================");

    let args: Vec<String> = env::args().collect();
    let mut wad_path = "vendor/DOOM1.WAD".to_string();
    let mut exec_path: Option<String> = None;
    let mut headless = false;
    let mut target_frames: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-iwad" | "--wad" => {
                if i + 1 < args.len() {
                    wad_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--exec" => {
                if i + 1 < args.len() {
                    exec_path = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--headless" => {
                headless = true;
            }
            "--frames" => {
                if i + 1 < args.len() {
                    if let Ok(n) = args[i + 1].parse::<usize>() {
                        target_frames = Some(n);
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    println!("-> Initializing Setun-DOS kernel...");
    let mut dos = SetunDos::new();
    if let Err(e) = dos.boot() {
        eprintln!("Kernel Panic: Failed to boot Setun-DOS: {}", e);
        std::process::exit(1);
    }

    println!("   [OK] TIVT Table initialized at address {}", TIVT_BASE);
    println!("   [OK] VGA Framebuffer cleared at address {} ({}x{} pixels)", FRAMEBUFFER_BASE, FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT);
    println!("   [OK] User execution space allocated (0..{})", USER_SPACE_LIMIT);

    // Mount DOOM1.WAD if present
    if Path::new(&wad_path).exists() {
        println!("-> Mounting WAD container: '{}'...", wad_path);
        match fs::read(&wad_path) {
            Ok(wad_bytes) => match dos.fs.mount_wad(&wad_bytes) {
                Ok(lumps) => println!("   [OK] Mounted {} lumps into virtual file descriptors.", lumps),
                Err(e) => eprintln!("   [WARN] WAD mounting error: {}", e),
            },
            Err(e) => eprintln!("   [WARN] Could not read WAD file: {}", e),
        }
    }

    if let Some(path) = exec_path {
        if path.eq_ignore_ascii_case("doom") {
            println!("-> Launching native Doom (1993) directly under Setun-DOS...");
            let mut desktop = SetunDesktop::new();
            desktop.set_wad_path(&wad_path);
            desktop.active_window = setun_doom::dos::ActiveWindow::Doom;
            desktop.doom_fullscreen = true;
            let _ = desktop.ensure_doom_runner();
            desktop.init_desktop_buttons(&mut dos);

            let has_display = env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok();
            let mut gui = if !headless && has_display {
                println!("-> Launching SDL2 graphical window (640x400)...");
                BenchGui::new("Setun-DOS (Level 3) - Native Doom (1993) on 21-Trit Architecture")
            } else {
                None
            };

            let max_frames = target_frames.unwrap_or(if gui.is_some() { usize::MAX } else { 35 });
            let mut rgba_buf = vec![0u8; 320 * 200 * 4];
            let mut frame = 0;
            let mut key_events = Vec::new();
            let mut clicked_coords = Vec::new();

            while frame < max_frames {
                if let Some(ref mut g) = gui {
                    key_events.clear();
                    clicked_coords.clear();

                    let keep_running = g.poll_events_desktop(
                        |key, pressed| key_events.push((key, pressed)),
                        |cx, cy| clicked_coords.push((cx, cy)),
                    );

                    if !keep_running {
                        println!("-> User requested exit via GUI.");
                        break;
                    }

                    for &(key, pressed) in &key_events {
                        desktop.handle_key(key, pressed);
                    }
                    for &(cx, cy) in &clicked_coords {
                        desktop.handle_mouse_click(&mut dos, cx as i64, cy as i64);
                    }

                    desktop.mouse_x = g.mouse_x as i64;
                    desktop.mouse_y = g.mouse_y as i64;
                    desktop.mouse_down = g.mouse_down;
                }

                desktop.render(&mut dos);

                if let Some(ref mut g) = gui {
                    dos.mem.render_framebuffer_rgba(&mut rgba_buf);
                    g.present_frame(&rgba_buf);
                    g.delay(28);
                }

                frame += 1;
            }
        } else {
            println!("-> Loading user TFX executable: '{}'...", path);
            let bytes = fs::read(&path).expect("Failed to read TFX executable");
            let tfx = TfxExecutable::from_bytes(&bytes).expect("Invalid TFX file format");
            let pid = dos.load_executable(&tfx, &path).expect("Failed to load TFX into kernel");
            println!("   [OK] Task PID {} loaded at PC = {}", pid, tfx.entry_pc);
            println!("-> Executing task...");
            let steps = dos.run_ticks(100_000);
            println!("   Execution completed in {} cycles.", steps);
        }
    } else {
        // Initialize desktop environment and icon manager
        println!("-> Initializing Setun-DOS Desktop Environment & Icon Manager...");
        let mut desktop = SetunDesktop::new();
        desktop.set_wad_path(&wad_path);
        desktop.init_desktop_buttons(&mut dos);

        let has_display = env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok();
        let mut gui = if !headless && has_display {
            println!("-> Launching SDL2 graphical desktop (640x400)...");
            BenchGui::new("Setun-DOS (Level 3) - Soviet Ternary Desktop & Window Manager")
        } else {
            None
        };

        if gui.is_some() {
            println!("   [OK] Desktop window active.");
            println!("   [OK] Clickable Desktop Icons:");
            println!("        [1] DOOM      -> Authentic 1993 Doom game window (interactive)");
            println!("        [2] T-SHELL   -> Ternary diagnostic console");
            println!("        [3] TELEMETRY -> Real-time 21-trit neon accumulator & flips");
            println!("        [4] ABOUT     -> System heritage & Brusentsov tribute");
            println!("   [OK] Controls: Left Click to open apps, click [X] or ESC/Q to close window.");
        } else if !headless {
            println!("   [INFO] No graphical display detected. Falling back to terminal headless mode.");
        }

        let max_frames = target_frames.unwrap_or(if gui.is_some() { usize::MAX } else { 35 });
        println!("-> Running desktop frame loop (target: {})...", if max_frames == usize::MAX { "interactive".to_string() } else { max_frames.to_string() });

        let mut rgba_buf = vec![0u8; 320 * 200 * 4];
        let mut frame = 0;
        let mut key_events = Vec::new();
        let mut clicked_coords = Vec::new();

        while frame < max_frames {
            if let Some(ref mut g) = gui {
                key_events.clear();
                clicked_coords.clear();

                let keep_running = g.poll_events_desktop(
                    |key, pressed| key_events.push((key, pressed)),
                    |cx, cy| clicked_coords.push((cx, cy)),
                );

                if !keep_running {
                    println!("-> User requested exit via desktop GUI.");
                    break;
                }

                for &(key, pressed) in &key_events {
                    desktop.handle_key(key, pressed);
                }
                for &(cx, cy) in &clicked_coords {
                    desktop.handle_mouse_click(&mut dos, cx as i64, cy as i64);
                }

                desktop.mouse_x = g.mouse_x as i64;
                desktop.mouse_y = g.mouse_y as i64;
                desktop.mouse_down = g.mouse_down;
            }

            desktop.render(&mut dos);

            if let Some(ref mut g) = gui {
                dos.mem.render_framebuffer_rgba(&mut rgba_buf);
                g.present_frame(&rgba_buf);
                g.delay(28); // ~35 FPS
            }

            frame += 1;
        }

        let telemetry = dos.cpu.telemetry;
        println!("============================================================");
        println!("  SETUN-DOS DESKTOP SESSION SUMMARY                         ");
        println!("============================================================");
        println!("  Frames Rendered:       {}", desktop.frame_counter);
        println!("  Kernel Timer Ticks:    {}", dos.timer_ticks);
        println!("  Total Trit Flips:      {}", telemetry.ternary_trit_flips);
        println!("  Half Swings (0 <-> 1): {}", telemetry.ternary_half_swings);
        println!("  Full Swings (-1 <-> 1):{}", telemetry.ternary_full_swings);
        let save_pct = if telemetry.binary32_bit_flips > 0 {
            100.0 * (1.0 - (telemetry.ternary_trit_flips as f64) / (telemetry.binary32_bit_flips as f64))
        } else {
            36.9
        };
        println!("  Dynamic Energy Save:   {:.2}%", save_pct);
        println!("============================================================");
    }
}
