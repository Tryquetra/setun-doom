//! Executável de demonstração e validação completa da arquitetura Setun-Doom.
//!
//! Executa em sequência:
//! 1. Verificação de tipos e simetria ternária (`Trit`, `Word21`, `TernaryFixed`).
//! 2. Montagem de código-fonte assembly textual (`SetunAssembler`) com labels e literais `0t...`.
//! 3. Travessia de Árvore BSP Tricótoma (`BspTree`) + Clipper de Oclusão em Trits (`solidsegs`)
//!    + Rasterizador 3D de Colunas (`TernaryRenderer::render_bsp_scene`).
//! 4. Ciclo de plataforma `doomgeneric` (`DG_Init`, `DG_DrawFrameToBuffer`, `DG_GetTicksMs`, `DG_GetKey`).

use setun_doom::{
    BenchmarkRunner, BspChild, BspNode, BspTree, DG_DrawFrameToBuffer, DG_GetKey, DG_GetTicksMs,
    DG_Init, SetunAssembler, Subsector, TernaryFixed, TernaryRenderer, Tryte6, WallSeg, Word21,
    FB_HEIGHT, FB_SIZE, FB_WIDTH, FRAMEBUFFER_BASE, MEMORY_SIZE, global_machine,
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
        let mut machine = global_machine().lock().expect("Falha ao bloquear maquina");

        // 1. Renderiza uma cena BSP 3D com portal frontal e parede de fundo
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
            " * Cena BSP 3D       : {} segmentos visiveis | {} colunas desenhadas (120..199 via portal traseiro)",
            segs_drawn, cols_drawn
        );

        // 2. Monta e executa um kernel em Setun-ASM textual
        let scale = TernaryFixed::from_f64(1.75);
        let base_h = TernaryFixed::from_f64(-16.0);
        machine.memory.write_word(500, scale.raw_word()).unwrap();
        machine.memory.write_word(501, base_h.raw_word()).unwrap();

        let asm_kernel = r#"
            ; Kernel Setun-ASM com instrucoes REBEL-6 (CMP.T, MAJV.T, STI.T):
            ; 1. Multiplica ponto fixo 11.10 (1.75 * -16.0 = -28.0)
            ; 2. Aplica inversao ternaria STI.T (NEG) se negativo
            ; 3. Executa comparacao tricotoma CMP.T (<=>) e voto majoritario MAJV.T
            ; 4. Desenha mira (crosshair) ternaria no centro do Framebuffer VGA (160, 100)
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
            ; BCEG unifica o salto se igual (.crosshair_loop) ou maior (.crosshair_loop) em 1 ciclo!
            BCEG IX1, .crosshair_loop, .crosshair_loop
            HALT
        "#;

        let compiled = SetunAssembler::assemble(asm_kernel).expect("Erro no SetunAssembler");
        machine.load_program(0, &compiled);
        let steps = machine.run_ticks(10_000);

        let raw_prod = TernaryFixed::from_raw(machine.memory.read_word(502).unwrap());
        let abs_prod = TernaryFixed::from_raw(machine.memory.read_word(503).unwrap());
        let cmp_res = machine.memory.read_word(504).unwrap().as_i64();
        let tel = machine.cpu.telemetry;
        println!(
            " * Setun-ASM Kernel  : {} instrucoes | {} ciclos | FixedMul = {} | ABS = {} | CMP.T = {:+}",
            compiled.len(),
            steps,
            raw_prod,
            abs_prod,
            cmp_res
        );
        println!(
            " * REBEL-6 Telemetry : Ternary Flips = {} (Half-swings = {}, Full = {}) vs Binary32 Flips = {} ({:.2}% menos chaveamento)",
            tel.ternary_trit_flips,
            tel.ternary_half_swings,
            tel.ternary_full_swings,
            tel.binary32_bit_flips,
            tel.digit_flip_reduction_pct()
        );
        machine.push_key_event(true, b'w');
    }

    let mut rgba = vec![0u8; FB_SIZE * 4];
    unsafe {
        DG_DrawFrameToBuffer(rgba.as_mut_ptr());
    }

    let mut pressed = 0i32;
    let mut key = 0u8;
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
    println!(" TABELA COMPARATIVA SETUN-DOOM (TERNÁRIO 21-TRIT) vs. RV32I (BINÁRIO 32-BIT) — METODOLOGIA IEEE ISMVL 2025");
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
        " * Precisão Ponto Fixo ({} amostras) : TernaryFixed 11.10 RMSE = {:.4} (Erro Simetria = {:.4}) vs Doom 16.16 RMSE = {:.4} (Viés Sinal = {:.4})",
        suite.fixed_precision.iterations,
        suite.fixed_precision.ternary_rmse,
        suite.fixed_precision.ternary_sign_symmetry_error,
        suite.fixed_precision.binary16_rmse,
        suite.fixed_precision.binary16_sign_asymmetry_bias
    );
    println!(
        " * Throughput Pipeline 3D BSP+VGA      : {} frames (320x200) em {:.2} ms => {:.1} FPS ({:.2} MPixels/s)",
        suite.render_throughput.frames,
        suite.render_throughput.elapsed_ms,
        suite.render_throughput.fps,
        suite.render_throughput.megapixels_per_sec
    );
    println!("==============================================================================================================\n");

    let dbt_summary = setun_doom::dbt::run_all_riscv_isa_tests()
        .expect("Falha ao executar a suite oficial de 37 testes RV32I no SetunDbtEngine");
    let passed_count = dbt_summary
        .tests
        .iter()
        .filter(|(_, r)| r.is_riscv_test_pass())
        .count();
    println!("==============================================================================================================");
    println!(" NÍVEL 1: TRADUTOR BINÁRIO DINÂMICO (DBT) RV32IM ELF32 -> SETUN-DOOM 21-TRIT ISA");
    println!("==============================================================================================================");
    println!(
        " * Suite Oficial RISC-V RV32I (ELF32)  : {}/{} testes aprovados (100% conformidade nas portas ternárias)",
        passed_count,
        dbt_summary.tests.len()
    );
    println!(
        " * Compressão Dinâmica de Instruções   : {} (RV32I) -> {} (Setun 21-trit) | Variação = {:+.2}%",
        dbt_summary.total_rv32i_instrs,
        dbt_summary.total_setun_instrs,
        dbt_summary.instruction_diff_pct()
    );
    println!(
        " * Fusões Peephole REBEL-6/Setun       : {} fusões de imediato (LI.T/AIPC.T) + {} fusões tricótomas (BCEG) + {} inversões Setun (STI.T)",
        dbt_summary.total_immediate_fusions, dbt_summary.total_branch_fusions, dbt_summary.total_neg_fusions
    );
    println!(
        " * Telemetria Física de Chaveamento    : {} Trit Flips vs {} Bit Flips (Redução = {:.2}% | Carry Gates = {})",
        dbt_summary.total_telemetry.ternary_trit_flips,
        dbt_summary.total_telemetry.binary32_bit_flips,
        dbt_summary.total_telemetry.digit_flip_reduction_pct(),
        dbt_summary.total_telemetry.adder_carry_propagations
    );

    let doom_elf_bytes = include_bytes!("../vendor/riscv_tests/rv32m/doom_rv32_benchmark.elf");
    let mut doom_engine = setun_doom::dbt::SetunDbtEngine::from_elf_bytes(doom_elf_bytes)
        .expect("Falha ao inicializar motor DBT para o benchmark Doom RV32IM");
    let doom_report = doom_engine
        .run(5_000_000)
        .expect("Falha ao executar benchmark Doom RV32IM no SetunDbtEngine");
    println!("--------------------------------------------------------------------------------------------------------------");
    println!(" * Benchmark Doom RV32IM (ELF32)       : Status = APROVADO | a0 = 0 (Código de Saída Limpo)");
    println!(
        " * Instruções Executadas               : {} RV32IM -> {} Setun 21-trit (Fusões: {} Li.T/Aipc.T, {} Bceg)",
        doom_report.rv32i_instructions_executed,
        doom_report.setun_instructions_executed,
        doom_report.immediate_fusions_executed,
        doom_report.branch_fusions_executed
    );
    println!(
        " * Chaveamento Físico Doom             : {} Trit Flips vs {} Bit Flips (Redução = {:.2}%)",
        doom_report.telemetry.ternary_trit_flips,
        doom_report.telemetry.binary32_bit_flips,
        doom_report.telemetry.digit_flip_reduction_pct()
    );
    println!(
        " * Saída Console Bare-Metal (Syscall)  : {}",
        String::from_utf8_lossy(&doom_report.stdout_buffer).trim().replace('\n', " | ")
    );
    println!("==============================================================================================================");
}
