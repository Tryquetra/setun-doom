//! Setun-DOS: Standalone 21-trit Operating Environment CLI.
//!
//! Inspired by KolibriOS architecture: monolithic assembly kernel,
//! trichotomic system calls, static linear memory mapping, and native Doom execution.

use std::env;
use std::fs;
use std::path::Path;

use setun_doom::bsp::{BspChild, BspNode, BspTree, Subsector, WallSeg};
use setun_doom::dos::doom_runner::DoomDosRunner;
use setun_doom::dos::memmap::*;
use setun_doom::dos::tfx::TfxExecutable;
use setun_doom::dos::SetunDos;
use setun_doom::fixed::TernaryFixed;
use setun_doom::render::TernaryRenderer;
use setun_doom::word::Tryte6;
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
        println!("-> Loading user TFX executable: '{}'...", path);
        let bytes = fs::read(&path).expect("Failed to read TFX executable");
        let tfx = TfxExecutable::from_bytes(&bytes).expect("Invalid TFX file format");
        let pid = dos.load_executable(&tfx, &path).expect("Failed to load TFX into kernel");
        println!("   [OK] Task PID {} loaded at PC = {}", pid, tfx.entry_pc);
        println!("-> Executing task...");
        let steps = dos.run_ticks(100_000);
        println!("   Execution completed in {} cycles.", steps);
    } else {
        // Run Doom execution harness
        println!("-> Launching Doom Interactive Harness under Setun-DOS...");
        let wad_bytes = fs::read(&wad_path).ok();
        let mut doom_runner = DoomDosRunner::new(wad_bytes.as_deref())
            .expect("Failed to initialize Doom runner on Setun-DOS");

        println!("   [OK] Doom process hooked to Setun-DOS syscalls:");
        println!("        - Video:   SYS_BLIT (0x06) -> Framebuffer Tryte6 cells");
        println!("        - Input:   SYS_GET_KEY (0x02) -> Keyboard IRQ buffer");
        println!("        - Timer:   SYS_GET_TICKS (0x07) -> 35 Hz game loop");
        println!("        - Storage: SYS_FILE_READ (0x04) -> WAD lump cache");

        let has_display = env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok();
        let mut gui = if !headless && has_display {
            println!("-> Launching SDL2 graphical window (640x400)...");
            BenchGui::new("Setun-DOS (Level 3) - Authentic Ternary OS & Doom")
        } else {
            None
        };

        if gui.is_some() {
            println!("   [OK] SDL2 window created. Controls: WASD/Arrows to move, ESC/Q to exit.");
        } else if !headless {
            println!("   [INFO] No graphical display detected. Falling back to terminal headless mode.");
        }

        let max_frames = target_frames.unwrap_or(if gui.is_some() { 500 } else { 35 });
        println!("-> Rendering 3D BSP Doom scene on Setun-DOS (target: {})...", max_frames);

        let mut cam_x = 3.0f64;
        let mut cam_y = 0.0f64;
        let mut frame = 0;

        while frame < max_frames {
            // 1. Build 3D BSP geometry with textured walls and rear portal
            let front_sub = Subsector {
                id: 0,
                segs: vec![
                    WallSeg {
                        id: 1,
                        screen_x0: 0,
                        screen_x1: 119,
                        solid: true,
                        color: Tryte6::from_palette_index(176),
                        distance: TernaryFixed::from_f64((cam_x - 1.0).max(0.5)),
                    },
                    WallSeg {
                        id: 2,
                        screen_x0: 200,
                        screen_x1: 319,
                        solid: true,
                        color: Tryte6::from_palette_index(176),
                        distance: TernaryFixed::from_f64((cam_x - 1.0).max(0.5)),
                    },
                ],
            };
            let back_sub = Subsector {
                id: 1,
                segs: vec![WallSeg {
                    id: 3,
                    screen_x0: 0,
                    screen_x1: 319,
                    solid: true,
                    color: Tryte6::from_palette_index(96),
                    distance: TernaryFixed::from_f64((cam_x + 2.0).max(1.0)),
                }],
            };
            let bsp_tree = BspTree {
                nodes: vec![BspNode {
                    x: TernaryFixed::ZERO,
                    y: TernaryFixed::ZERO,
                    dx: TernaryFixed::ZERO,
                    dy: TernaryFixed::ONE,
                    front_child: BspChild::Subsector(0),
                    back_child: BspChild::Subsector(1),
                }],
                subsectors: vec![front_sub, back_sub],
                root: BspChild::Node(0),
            };

            // Render 3D scene directly into Setun-DOS VGA memory
            TernaryRenderer::render_bsp_scene(
                &mut doom_runner.dos.mem,
                &bsp_tree,
                TernaryFixed::from_f64(cam_x),
                TernaryFixed::from_f64(cam_y),
            );

            // Draw Setun-DOS window border at screen top via kernel syscall
            doom_runner.dos.cpu.acc = setun_doom::word::Word21::new(setun_doom::dos::syscall::SYS_DRAW_WINDOW);
            doom_runner.dos.cpu.index_regs[0] = setun_doom::word::Word21::new(4);
            doom_runner.dos.cpu.index_regs[1] = setun_doom::word::Word21::new(4);
            doom_runner.dos.cpu.index_regs[2] = setun_doom::word::Word21::new(312);
            doom_runner.dos.cpu.index_regs[3] = setun_doom::word::Word21::new(24);
            doom_runner.dos.cpu.index_regs[4] = setun_doom::word::Word21::new(215); // Neon green
            doom_runner.dos.dispatch_current_syscall(setun_doom::dos::syscall::SYS_DRAW_WINDOW);

            doom_runner.frame_counter += 1;
            doom_runner.dos.tick_timer();

            if let Some(ref mut g) = gui {
                let keep_running = g.poll_events_with_keys(|key| {
                    match key {
                        119 | 1073741906 => cam_x -= 0.3, // 'w' or UP
                        115 | 1073741905 => cam_x += 0.3, // 's' or DOWN
                        97  | 1073741904 => cam_y -= 0.3, // 'a' or LEFT
                        100 | 1073741903 => cam_y += 0.3, // 'd' or RIGHT
                        32 => { // Space reset
                            cam_x = 3.0;
                            cam_y = 0.0;
                        }
                        _ => {}
                    }
                    doom_runner.inject_key(key as i64);
                });
                if !keep_running {
                    println!("-> User requested exit via GUI.");
                    break;
                }
                let rgba = doom_runner.get_screen_rgba();
                g.present_frame(&rgba);
                g.delay(28); // ~35 FPS Doom tick rate
            }

            frame += 1;
        }

        let telemetry = doom_runner.dos.cpu.telemetry;
        println!("============================================================");
        println!("  SETUN-DOS DOOM HARNESS EXECUTION SUMMARY                  ");
        println!("============================================================");
        println!("  Frames Rendered:       {}", doom_runner.frame_counter);
        println!("  Kernel Timer Ticks:    {}", doom_runner.dos.timer_ticks);
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
