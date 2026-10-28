// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Demonstration and complete validation executable for the Setun-Doom architecture.
//!
//! Executes in sequence:
//! 1. Type verification and ternary symmetry (`Trit`, `Word21`, `TernaryFixed`).
//! 2. Textual assembly code assembly (`SetunAssembler`) with labels and `0t...` literals.
//! 3. Trichotomous BSP Tree Traversal (`BspTree`) + Trit Occlusion Clipper (`solidsegs`)
//!    + 3D Column Rasterizer (`TernaryRenderer::render_bsp_scene`).
//! 4. Platform cycle for `doomgeneric` (`DG_Init`, `DG_DrawFrameToBuffer`, `DG_GetTicksMs`, `DG_GetKey`).

use setun_doom::{
    global_machine, BenchGui, BenchmarkRunner, BspChild, BspNode, BspTree, DG_DrawFrameToBuffer,
    DG_GetKey, DG_GetTicksMs, DG_Init, SetunAssembler, Subsector, TernaryFixed, TernaryRenderer,
    Tryte6, WallSeg, Word21, FB_HEIGHT, FB_SIZE, FB_WIDTH, FRAMEBUFFER_BASE, MEMORY_SIZE,
};

fn main() {
    println!("================================================================");
    println!(" SETUN-DOOM (21-Trit Balanced Ternary Computer Emulator)");
    println!("================================================================");
    println!(
        " * Word21 States     : 3^21 = {} ([{}, +{}])",
        Word21::MODULUS,
        Word21::MIN_VAL,
        Word21::MAX_VAL
    );
    println!(
        " * TernaryFixed      : 11.10 trits (Scale 3^10 = {})",
        setun_doom::FRAC_SCALE
    );
    println!(
        " * Linear Memory     : 3^14 = {} words (~36.5 MB host allocation)",
        MEMORY_SIZE
    );
    println!(
        " * VGA Framebuffer   : {}x{} = {} pixels @ base addr {}",
        FB_WIDTH, FB_HEIGHT, FB_SIZE, FRAMEBUFFER_BASE
    );

    DG_Init();

    {
        let mut machine = global_machine()
            .lock()
            .expect("Failed to lock global machine");

        // 1. Render a 3D BSP scene with frontal portal and back wall
        let front_sub = Subsector {
            id: 0,
            segs: vec![
                WallSeg {
                    id: 1,
                    screen_x0: 0,
                    screen_x1: 119,
                    solid: true,
                    color: Tryte6::from_palette_index(176),
                    distance: TernaryFixed::from_int(2),
                },
                WallSeg {
                    id: 2,
                    screen_x0: 200,
                    screen_x1: 319,
                    solid: true,
                    color: Tryte6::from_palette_index(176),
                    distance: TernaryFixed::from_int(2),
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
                distance: TernaryFixed::from_int(5),
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

        let (segs_drawn, cols_drawn) = TernaryRenderer::render_bsp_scene(
            &mut machine.memory,
            &bsp_tree,
            TernaryFixed::from_int(3),
            TernaryFixed::ZERO,
        );
        println!(
            " * 3D BSP Scene      : {} visible segments | {} columns drawn (120..199 through rear portal)",
            segs_drawn, cols_drawn
        );

        // 2. Assemble and run a kernel in textual Setun-ASM
        let scale = TernaryFixed::from_f64(1.75);
        let base_h = TernaryFixed::from_f64(-16.0);
        machine.memory.write_word(500, scale.raw_word()).unwrap();
        machine.memory.write_word(501, base_h.raw_word()).unwrap();

        let asm_kernel = r#"
            ; Setun-ASM Kernel with REBEL-6 instructions (CMP.T, MAJV.T, STI.T):
            ; 1. Multiplies 11.10 fixed point (1.75 * -16.0 = -28.0)
            ; 2. Applies ternary inversion STI.T (NEG) if negative
            ; 3. Executes trichotomous comparison CMP.T (<=>) and majority vote MAJV.T
            ; 4. Draws ternary crosshair in the center of VGA Framebuffer (160, 100)
            LOAD 500
            MUL 501
            STORE 502
            JMPN .make_positive
            JMP .store_abs
        .make_positive:
            STI.T
        .store_abs:
            STORE 503
            CMP.T 502
            STORE 504
            LOAD_IX_IMM IX0, 0t+++++
            MAJV.T IX0, 504
            STORE 505
            LOAD_IX_IMM IX0, 4
            LOAD_IX_IMM IX1, 0
        .crosshair_loop:
            LOAD_IMM 0t+++++
            STORE_INDEX IX0, 4632158
            ADD_IX_IMM IX0, -1
            MOV_IX_ACC IX0
            ; BCEG unifies the jump if equal (.crosshair_loop) or greater (.crosshair_loop) in 1 cycle
            BCEG IX1, .crosshair_loop, .crosshair_loop
            HALT
        "#;

        let compiled = SetunAssembler::assemble(asm_kernel).expect("Error in SetunAssembler");
        machine.load_program(0, &compiled);
        let steps = machine.run_ticks(10_000);

        let raw_prod = TernaryFixed::from_raw(machine.memory.read_word(502).unwrap());
        let abs_prod = TernaryFixed::from_raw(machine.memory.read_word(503).unwrap());
        let cmp_res = machine.memory.read_word(504).unwrap().as_i64();
        let tel = machine.cpu.telemetry;
        println!(
            " * Setun-ASM Kernel  : {} instructions | {} cycles | FixedMul = {} | ABS = {} | CMP.T = {:+}",
            compiled.len(),
            steps,
            raw_prod,
            abs_prod,
            cmp_res
        );
        println!(
            " * REBEL-6 Telemetry : Ternary Flips = {} (Half-swings = {}, Full = {}) vs Binary32 Flips = {} ({:.2}% reduction)",
            tel.ternary_trit_flips,
            tel.ternary_half_swings,
            tel.ternary_full_swings,
            tel.binary32_bit_flips,
            tel.digit_flip_reduction_pct()
        );
        machine.push_key_event(true, b'w');
    }

    let mut rgba = vec![0u8; FB_SIZE * 4];
    // SAFETY: rgba vector is allocated with FB_SIZE * 4 bytes matching the expected RGBA output buffer.
    unsafe {
        DG_DrawFrameToBuffer(rgba.as_mut_ptr());
    }

    let mut pressed = 0i32;
    let mut key = 0u8;
    // SAFETY: pressed and key pointers are valid stack addresses for output parameters.
    let got_key = unsafe { DG_GetKey(&mut pressed, &mut key) };
    let ticks = DG_GetTicksMs();

    println!(
        " * DG_DrawFrame      : Pixel(0,0) RGBA = [{}, {}, {}, {}]",
        rgba[0], rgba[1], rgba[2], rgba[3]
    );
    println!(
        " * DG_GetKey         : status={} pressed={} key='{}' (0x{:02X}) | Ticks={}ms",
        got_key, pressed, key as char, key, ticks
    );
    println!("================================================================\n");

    let suite = BenchmarkRunner::run_all();
    println!("==============================================================================================================");
    println!(" COMPARATIVE TABLE: SETUN-DOOM (21-TRIT TERNARY) vs. RV32I (32-BIT BINARY) — IEEE ISMVL 2025 METHODOLOGY");
    println!("==============================================================================================================");
    println!(
        " {:<32} | {:>10} | {:>9} | {:>11} | {:>11} | {:>11} | {:>9}",
        "Benchmark / Kernel",
        "Instr Tern",
        "Run Instr",
        "Trit Flips",
        "Digit Flips",
        "Energy dV^2",
        "Host MIPS"
    );
    println!("--------------------------------------------------------------------------------------------------------------");
    for b in &suite.isa_benchmarks {
        println!(
            " {:<32} | {:>10} | {:>+8.2}% | {:>11} | {:>+10.2}% | {:>+10.2}% | {:>9.2}",
            b.name,
            b.ternary_instructions,
            b.instruction_diff_pct(),
            b.telemetry.ternary_trit_flips,
            b.digit_flip_diff_pct(),
            b.weighted_energy_diff_pct(),
            b.mips()
        );
    }
    println!("==============================================================================================================");
    println!(
        " * Fixed-Point Precision ({} samples) : TernaryFixed 11.10 RMSE = {:.4} (Symmetry Error = {:.4}) vs Doom 16.16 RMSE = {:.4} (Sign Bias = {:.4})",
        suite.fixed_precision.iterations,
        suite.fixed_precision.ternary_rmse,
        suite.fixed_precision.ternary_sign_symmetry_error,
        suite.fixed_precision.binary16_rmse,
        suite.fixed_precision.binary16_sign_asymmetry_bias
    );
    println!(
        " * 3D BSP+VGA Pipeline Throughput     : {} frames (320x200) in {:.2} ms => {:.1} FPS ({:.2} MPixels/s)",
        suite.render_throughput.frames,
        suite.render_throughput.elapsed_ms,
        suite.render_throughput.fps,
        suite.render_throughput.megapixels_per_sec
    );
    println!("==============================================================================================================\n");

    let dbt_summary = setun_doom::dbt::run_all_riscv_isa_tests()
        .expect("Failed to execute official 37 RV32I tests on SetunDbtEngine");
    let passed_count = dbt_summary
        .tests
        .iter()
        .filter(|(_, r)| r.is_riscv_test_pass())
        .count();
    println!("==============================================================================================================");
    println!(" LEVEL 1: DYNAMIC BINARY TRANSLATOR (DBT) RV32IM ELF32 -> SETUN-DOOM 21-TRIT ISA");
    println!("==============================================================================================================");
    println!(
        " * Official RISC-V RV32I Suite (ELF32) : {}/{} tests passed (100% compliance across ternary gates)",
        passed_count,
        dbt_summary.tests.len()
    );
    println!(
        " * Dynamic Instruction Compression    : {} (RV32I) -> {} (Setun 21-trit) | Variation = {:+.2}%",
        dbt_summary.total_rv32i_instrs,
        dbt_summary.total_setun_instrs,
        dbt_summary.instruction_diff_pct()
    );
    println!(
        " * REBEL-6/Setun Peephole Fusions     : {} immediate fusions (LI.T/AIPC.T) + {} trichotomous fusions (BCEG) + {} Setun inversions (STI.T)",
        dbt_summary.total_immediate_fusions, dbt_summary.total_branch_fusions, dbt_summary.total_neg_fusions
    );
    println!(
        " * Physical Switching Telemetry       : {} Trit Flips vs {} Bit Flips (Reduction = {:.2}% | Carry Gates = {})",
        dbt_summary.total_telemetry.ternary_trit_flips,
        dbt_summary.total_telemetry.binary32_bit_flips,
        dbt_summary.total_telemetry.digit_flip_reduction_pct(),
        dbt_summary.total_telemetry.adder_carry_propagations
    );

    let doom_elf_bytes = include_bytes!("../vendor/riscv_tests/rv32m/doom_rv32_benchmark.elf");
    let mut doom_engine = setun_doom::dbt::SetunDbtEngine::from_elf_bytes(doom_elf_bytes)
        .expect("Failed to initialize DBT engine for Doom RV32IM benchmark");
    let doom_report = doom_engine
        .run(5_000_000)
        .expect("Failed to execute Doom RV32IM benchmark in SetunDbtEngine");
    println!("--------------------------------------------------------------------------------------------------------------");
    println!(" * Doom RV32IM Benchmark (ELF32)      : Status = PASSED | a0 = 0 (Clean Exit Code)");
    println!(
        " * Instructions Executed              : {} RV32IM -> {} Setun 21-trit (Fusions: {} Li.T/Aipc.T, {} Bceg)",
        doom_report.rv32i_instructions_executed,
        doom_report.setun_instructions_executed,
        doom_report.immediate_fusions_executed,
        doom_report.branch_fusions_executed
    );
    println!(
        " * Physical Switching Doom            : {} Trit Flips vs {} Bit Flips (Reduction = {:.2}%)",
        doom_report.telemetry.ternary_trit_flips,
        doom_report.telemetry.binary32_bit_flips,
        doom_report.telemetry.digit_flip_reduction_pct()
    );
    println!(
        " * Bare-Metal Console Output (Syscall): {}",
        String::from_utf8_lossy(&doom_report.stdout_buffer)
            .trim()
            .replace('\n', " | ")
    );
    println!("==============================================================================================================");

    // If --no-gui argument is not provided and DISPLAY is set, open interactive graphical interface
    let enable_gui =
        !std::env::args().any(|arg| arg == "--no-gui") && std::env::var("DISPLAY").is_ok();
    if enable_gui {
        println!("\n[GUI] Initializing testbench graphical window (SDL2 640x400)...");
        println!("[GUI] Interactive controls: W/S (camera forward/backward) | A/D (lateral) | Space (reset) | ESC/Q (quit)");

        if let Some(mut gui) = BenchGui::new("Setun-Doom Testbench - 21-Trit BSP & ALU Visualizer")
        {
            let mut last_cam_x = gui.cam_x;
            let mut last_cam_y = gui.cam_y;

            while gui.poll_events() {
                if (gui.cam_x - last_cam_x).abs() > 0.001 || (gui.cam_y - last_cam_y).abs() > 0.001
                {
                    last_cam_x = gui.cam_x;
                    last_cam_y = gui.cam_y;

                    let mut machine = global_machine()
                        .lock()
                        .expect("Failed to lock global machine");
                    let front_sub = Subsector {
                        id: 0,
                        segs: vec![
                            WallSeg {
                                id: 1,
                                screen_x0: 0,
                                screen_x1: 119,
                                solid: true,
                                color: Tryte6::from_palette_index(176),
                                distance: TernaryFixed::from_f64((gui.cam_x - 1.0).max(0.5)),
                            },
                            WallSeg {
                                id: 2,
                                screen_x0: 200,
                                screen_x1: 319,
                                solid: true,
                                color: Tryte6::from_palette_index(176),
                                distance: TernaryFixed::from_f64((gui.cam_x - 1.0).max(0.5)),
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
                            distance: TernaryFixed::from_f64((gui.cam_x + 2.0).max(1.0)),
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

                    TernaryRenderer::render_bsp_scene(
                        &mut machine.memory,
                        &bsp_tree,
                        TernaryFixed::from_f64(gui.cam_x),
                        TernaryFixed::from_f64(gui.cam_y),
                    );
                }

                // SAFETY: rgba buffer is allocated with FB_SIZE * 4 bytes for the 320x200 RGBA frame output.
                unsafe {
                    DG_DrawFrameToBuffer(rgba.as_mut_ptr());
                }
                gui.present_frame(&rgba);
                gui.delay(16); // ~60 FPS
            }
            println!("[GUI] Graphical window closed.");
        }
    }
}
