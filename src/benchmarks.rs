//! # Suíte de Benchmarks Científicos e de Motor Gráfico (`Setun-Doom` vs. `RV32I` Binário)
//!
//! Implementa os experimentos comparativos entre a arquitetura ternária balanceada (`Setun-Doom` /
//! `REBEL-6`) e a referência binária de 32 bits em complemento de dois (`RV32I`):
//!
//! 1. **Tabela Comparativa de Carga de Trabalho (Estilo Tabela IV do Artigo REBEL-6)**:
//!    - **Busca de 3 Vias com `BCEG`** (*Three-Way Search* em array ordenado na memória ternária).
//!    - **Particionamento Quicksort de 3 Vias** (*Dutch National Flag* de Dijkstra usando `CMP.T` e `BCEG`).
//!    - **Oscilação de Sinal & Travessia de Plano BSP** (produto vetorial cruzando o zero repetidamente).
//!    - **Interpolador de Colunas `R_DrawColumn`** (acumulação de coordenadas de textura no Framebuffer VGA).
//!    - **Micro-Dhrystone Ternário** (carga intensiva de milhões de ciclos para medir MIPS reais no host).
//! 2. **Análise de Precisão Numérica (Drift / RMSE)**:
//!    - Compara o erro acumulado do ponto fixo ternário `11.10` ([`TernaryFixed`], arredondamento simétrico)
//!      contra o `fixed_t` binário `16.16` do Doom (truncamento assimétrico para $-\infty$) frente a `f64`.
//! 3. **Throughput de Renderização 3D (BSP + Occlusion Clipper + RGBA)**:
//!    - Mede quadros por segundo (FPS) equivalentes e milhões de pixels/segundo no pipeline completo.

use crate::assembler::SetunAssembler;
use crate::bsp::{BspChild, BspNode, BspTree, Subsector, WallSeg};
use crate::cpu::{Cpu, PowerTelemetry};
use crate::fixed::{TernaryFixed, DOOM_BINARY_FRACUNIT};
use crate::memory::{Memory, FB_SIZE};
use crate::render::TernaryRenderer;
use crate::word::{Tryte6, Word21};
use std::time::Instant;

/// Resultado consolidado de um benchmark individual de ISA (Estilo Tabela IV do IEEE ISMVL 2025).
#[derive(Clone, Debug)]
pub struct IsaBenchmarkResult {
    /// Nome do benchmark.
    pub name: &'static str,
    /// Instruções executadas na nossa ISA ternária (`Setun-Doom`).
    pub ternary_instructions: u64,
    /// Instruções equivalentes exigidas em `RV32I` binário (onde cada `BCEG` exige `blt + beq`
    /// e constantes de 21 trits exigem `lui + addi`).
    pub binary_rv32i_instructions: u64,
    /// Telemetria de chaveamento de estados (*Trit Flips*, *Half-Swings*, *Full-Swings* e *Bit Flips*).
    pub telemetry: PowerTelemetry,
    /// Tempo de parede gasto na simulação em microssegundos.
    pub elapsed_us: u128,
}

impl IsaBenchmarkResult {
    /// Diferença percentual de instruções executadas (`< 0.0` indica menos instruções no ternário).
    pub fn instruction_diff_pct(&self) -> f64 {
        if self.binary_rv32i_instructions == 0 {
            return 0.0;
        }
        let b = self.binary_rv32i_instructions as f64;
        let t = self.ternary_instructions as f64;
        ((t - b) / b) * 100.0
    }

    /// Diferença percentual de *Digit Flips* (`< 0.0` indica menos chaveamento no ternário).
    pub fn digit_flip_diff_pct(&self) -> f64 {
        -self.telemetry.digit_flip_reduction_pct()
    }

    /// Diferença percentual de energia capacitiva ponderada ($\Delta V^2$, onde *Half-Swing* = 0.25x).
    pub fn weighted_energy_diff_pct(&self) -> f64 {
        -self.telemetry.weighted_energy_reduction_pct()
    }

    /// Milhões de instruções ternárias executadas por segundo (MIPS) no simulador Rust.
    pub fn mips(&self) -> f64 {
        if self.elapsed_us == 0 {
            return 0.0;
        }
        (self.ternary_instructions as f64) / (self.elapsed_us as f64)
    }
}

/// Resultado do experimento de precisão numérica (`TernaryFixed 11.10` vs. `Doom Binary 16.16`).
#[derive(Copy, Clone, Debug)]
pub struct FixedPrecisionReport {
    pub iterations: usize,
    pub ternary_rmse: f64,
    pub binary16_rmse: f64,
    pub ternary_sign_symmetry_error: f64,
    pub binary16_sign_asymmetry_bias: f64,
}

/// Resultado de performance de renderização 3D + tradução RGBA.
#[derive(Copy, Clone, Debug)]
pub struct RenderThroughputReport {
    pub frames: usize,
    pub elapsed_ms: f64,
    pub fps: f64,
    pub megapixels_per_sec: f64,
}

/// Relatório global da suíte de benchmarks Setun-Doom.
pub struct FullBenchmarkSuiteReport {
    pub isa_benchmarks: Vec<IsaBenchmarkResult>,
    pub fixed_precision: FixedPrecisionReport,
    pub render_throughput: RenderThroughputReport,
}

/// Executor de referência `RV32I` real com banco de 32 registradores (`x0..x31` em complemento de dois).
/// Elimina estimativas estáticas (`cpu.cycles + N`): cada benchmark executa o algoritmo equivalente
/// diretamente nos 32 registradores do `RV32I` (sem penalidade de acumulador `MOV_IX_ACC`),
/// contabilizando instruções reais e *Bit Flips* exatos em `x0..x31`.
#[derive(Clone, Debug, Default)]
pub struct Rv32iReferenceCpu {
    pub regs: [i32; 32],
    pub instructions: u64,
    pub bit_flips: u64,
}

impl Rv32iReferenceCpu {
    pub fn new() -> Self {
        Self::default()
    }

    /// Escreve em `regs[rd]` (com `x0` fixo em 0) e soma a distância de Hamming `(prev ^ next).count_ones()`.
    #[inline(always)]
    pub fn write_reg(&mut self, rd: usize, val: i32) {
        self.instructions += 1;
        if rd == 0 {
            return;
        }
        let prev = self.regs[rd];
        if prev != val {
            self.bit_flips += (prev ^ val).count_ones() as u64;
            self.regs[rd] = val;
        }
    }

    /// Contabiliza uma instrução que não altera registradores (ex: `beq`, `blt`, `bge`, `j`, `sw`).
    #[inline(always)]
    pub fn step_branch_or_store(&mut self) {
        self.instructions += 1;
    }
}

/// Executor da bateria de benchmarks científicos e de motor gráfico.
pub struct BenchmarkRunner;

impl BenchmarkRunner {
    /// Executa todos os benchmarks e retorna o relatório consolidado.
    pub fn run_all() -> FullBenchmarkSuiteReport {
        let mut isa_benchmarks = Vec::new();
        isa_benchmarks.push(Self::bench_three_way_search_bceg());
        isa_benchmarks.push(Self::bench_dutch_flag_3way_partition());
        isa_benchmarks.push(Self::bench_bsp_zero_crossing_determinant());
        isa_benchmarks.push(Self::bench_r_draw_column_kernel());
        isa_benchmarks.push(Self::bench_dhrystone_stress_2m_cycles());

        let fixed_precision = Self::bench_fixed_point_precision();
        let render_throughput = Self::bench_bsp_and_vga_throughput(120);

        FullBenchmarkSuiteReport {
            isa_benchmarks,
            fixed_precision,
            render_throughput,
        }
    }

    /// 1. Benchmark de Busca Ordenada de 3 Vias usando `BCEG` (`Three-Way Search`).
    /// Compara a execução real na CPU `Setun-Doom` contra a execução real no banco de 32 registradores `RV32I`.
    fn bench_three_way_search_bceg() -> IsaBenchmarkResult {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        for i in 0..256 {
            let val = ((i as i64) - 128) * 10;
            mem.write_word_fast(1000 + (i as i64), Word21::new(val));
        }

        let asm = r#"
            LOAD_IX_IMM IX0, 0          ; índice atual no vetor (0..255)
            LOAD_IX_IMM IX1, 700        ; chave alvo (+700 -> índice 198)
            LOAD_IMM 0
            STORE 900                   ; contador de passos
        .search_loop:
            LOAD_INDEX IX0, 1000        ; ACC = array[IX0]
            BCEG IX1, .found, .overshot
            ADD_IX_IMM IX0, 1
            JMP .search_loop
        .overshot:
            ADD_IX_IMM IX0, -1
            JMP .search_loop
        .found:
            MOV_IX_ACC IX0
            STORE 901                   ; salva índice encontrado (198)
            HALT
        "#;

        let prog = SetunAssembler::assemble(asm).unwrap();
        cpu.load_program(&mut mem, 0, &prog);

        let t0 = Instant::now();
        cpu.run_ticks(&mut mem, 50_000);
        let elapsed_us = t0.elapsed().as_micros().max(1);

        assert!(cpu.halted && cpu.trap.is_none());
        assert_eq!(mem.read_word_fast(901).as_i64(), 198);

        // Executa o mesmo algoritmo no processador RV32I de 32 registradores:
        // x1 = idx (0), x2 = target (700), x3 = array[idx]
        let mut rv = Rv32iReferenceCpu::new();
        rv.write_reg(1, 0); // li x1, 0
        rv.write_reg(2, 700); // li x2, 700
        rv.step_branch_or_store(); // sw x0, 900(x0)
        loop {
            let idx = rv.regs[1];
            let elem = ((idx - 128) * 10) as i32;
            rv.write_reg(3, elem); // lw x3, 1000(x1)
            rv.step_branch_or_store(); // beq x3, x2, .found
            if elem == rv.regs[2] {
                break;
            }
            rv.step_branch_or_store(); // bgt x3, x2, .overshot
            if elem > rv.regs[2] {
                rv.write_reg(1, idx - 1); // addi x1, x1, -1
                rv.step_branch_or_store(); // j .search_loop
            } else {
                rv.write_reg(1, idx + 1); // addi x1, x1, 1
                rv.step_branch_or_store(); // j .search_loop
            }
        }
        rv.step_branch_or_store(); // sw x1, 901(x0)
        rv.step_branch_or_store(); // ret / ebreak
        assert_eq!(rv.regs[1], 198);

        let mut tel = cpu.telemetry;
        tel.binary32_bit_flips = rv.bit_flips;

        IsaBenchmarkResult {
            name: "1. Busca 3-Vias (BCEG Search)",
            ternary_instructions: cpu.cycles,
            binary_rv32i_instructions: rv.instructions,
            telemetry: tel,
            elapsed_us,
        }
    }

    /// 2. Benchmark de Particionamento de 3 Vias de Dijkstra (*Dutch National Flag* / Quicksort Ternário).
    /// Compara acumulador ternário + `BCEG` contra `RV32I` de 32 registradores (onde `RV32I` não paga `MOV_IX_ACC`).
    fn bench_dutch_flag_3way_partition() -> IsaBenchmarkResult {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        for i in 0..300 {
            let v = match i % 3 {
                0 => -((i as i64) + 1),
                1 => 0,
                _ => (i as i64) + 1,
            };
            mem.write_word_fast(2000 + (i as i64), Word21::new(v));
        }

        let asm = r#"
            LOAD_IX_IMM IX0, 299        ; cursor de varredura (299..0)
            LOAD_IX_IMM IX1, 0          ; pivô = 0
            LOAD_IX_IMM IX2, 0          ; contagem < 0
            LOAD_IX_IMM IX3, 0          ; contagem == 0
            LOAD_IX_IMM IX4, 0          ; contagem > 0
        .part_loop:
            LOAD_INDEX IX0, 2000        ; ACC = vetor[IX0]
            BCEG IX1, .is_zero, .is_pos
            ADD_IX_IMM IX2, 1
            JMP .next_elem
        .is_zero:
            ADD_IX_IMM IX3, 1
            JMP .next_elem
        .is_pos:
            ADD_IX_IMM IX4, 1
        .next_elem:
            BIX IX0, .part_loop         ; IX0 -= 1; se IX0 >= 0 salta sem destruir ACC!
            HALT
        "#;

        let prog = SetunAssembler::assemble(asm).unwrap();
        cpu.load_program(&mut mem, 0, &prog);

        let t0 = Instant::now();
        cpu.run_ticks(&mut mem, 100_000);
        let elapsed_us = t0.elapsed().as_micros().max(1);

        assert!(cpu.halted && cpu.trap.is_none());
        assert_eq!(cpu.index_regs[2].as_i64(), 100);
        assert_eq!(cpu.index_regs[3].as_i64(), 100);
        assert_eq!(cpu.index_regs[4].as_i64(), 100);

        // Execução real em RV32I com 32 registradores (x1=i, x2=neg, x3=zero, x4=pos, x5=elem):
        // Note que em RV32I o fim do loop usa `addi x1, x1, -1; bge x1, x0, .part_loop` sem precisar de MOV_IX_ACC!
        let mut rv = Rv32iReferenceCpu::new();
        rv.write_reg(1, 299); // li x1, 299
        rv.write_reg(2, 0); // li x2, 0
        rv.write_reg(3, 0); // li x3, 0
        rv.write_reg(4, 0); // li x4, 0
        loop {
            let i = rv.regs[1] as usize;
            let elem = mem.read_word_fast(2000 + (i as i64)).as_i64() as i32;
            rv.write_reg(5, elem); // lw x5, 2000(x1)
            rv.step_branch_or_store(); // beq x5, x0, .is_zero
            if elem == 0 {
                let c = rv.regs[3];
                rv.write_reg(3, c + 1); // addi x3, x3, 1
                rv.step_branch_or_store(); // j .next_elem
            } else {
                rv.step_branch_or_store(); // bgt x5, x0, .is_pos
                if elem > 0 {
                    let c = rv.regs[4];
                    rv.write_reg(4, c + 1); // addi x4, x4, 1
                } else {
                    let c = rv.regs[2];
                    rv.write_reg(2, c + 1); // addi x2, x2, 1
                    rv.step_branch_or_store(); // j .next_elem
                }
            }
            let next_i = rv.regs[1] - 1;
            rv.write_reg(1, next_i); // addi x1, x1, -1
            rv.step_branch_or_store(); // bge x1, x0, .part_loop
            if next_i < 0 {
                break;
            }
        }
        rv.step_branch_or_store(); // ret
        assert_eq!(rv.regs[2], 100);
        assert_eq!(rv.regs[3], 100);
        assert_eq!(rv.regs[4], 100);

        let mut tel = cpu.telemetry;
        tel.binary32_bit_flips = rv.bit_flips;

        IsaBenchmarkResult {
            name: "2. Quicksort 3-Way Partition",
            ternary_instructions: cpu.cycles,
            binary_rv32i_instructions: rv.instructions,
            telemetry: tel,
            elapsed_us,
        }
    }

    /// 3. Benchmark de Travessia de Plano BSP Real (`ROM_POINT_ON_SIDE` do Doom).
    /// Executa a sub-rotina de firmware `ROM_POINT_ON_SIDE_ENTRY` (`R_PointOnSide` completo com
    /// `SUB`, `SHIFT -10`, `MUL`, `CMP_T`, `JMP_N`) para 200 pontos oscilando entre frente e trás
    /// de um plano divisor BSP, comparado contra `R_PointOnSide` em `RV32I` (`16.16` binário).
    fn bench_bsp_zero_crossing_determinant() -> IsaBenchmarkResult {
        let mut machine = crate::mmo::SetunDoomMachine::new();
        let mut rv = Rv32iReferenceCpu::new();

        // Plano divisor diagonal: lx = 100.0, ly = -50.0, ldx = 24.0, ldy = -18.0
        let lx16 = 100 * 65536;
        let ly16 = -50 * 65536;
        let ldx16 = 24 * 65536;
        let ldy16 = -18 * 65536;

        let tlx = TernaryFixed::from_doom_fixed16(lx16).raw_word();
        let tly = TernaryFixed::from_doom_fixed16(ly16).raw_word();
        let tldx = TernaryFixed::from_doom_fixed16(ldx16).raw_word();
        let tldy = TernaryFixed::from_doom_fixed16(ldy16).raw_word();

        machine.memory.write_word_fast(62, tlx);
        machine.memory.write_word_fast(63, tly);
        machine.memory.write_word_fast(64, tldx);
        machine.memory.write_word_fast(65, tldy);

        // Inicializa registradores constantes no RV32I: x10=lx, x11=ly, x12=ldx, x13=ldy
        rv.write_reg(10, lx16);
        rv.write_reg(11, ly16);
        rv.write_reg(12, ldx16);
        rv.write_reg(13, ldy16);

        let t0 = Instant::now();
        for i in 0..200i32 {
            // Oscila o ponto (x, y) cruzando o plano divisor a cada iteração
            let sign = if i % 2 == 0 { 1 } else { -1 };
            let x16 = (100 + sign * ((i % 37) + 1)) * 65536 + (sign * 16384);
            let y16 = (-50 - sign * ((i % 29) + 1)) * 65536 - (sign * 8192);

            // 1. Executa ROM_POINT_ON_SIDE_ENTRY na CPU Setun-Doom
            machine
                .memory
                .write_word_fast(60, TernaryFixed::from_doom_fixed16(x16).raw_word());
            machine
                .memory
                .write_word_fast(61, TernaryFixed::from_doom_fixed16(y16).raw_word());
            machine.execute_rom_routine(crate::mmo::ROM_POINT_ON_SIDE_ENTRY, 32);
            let side_ternary = machine.memory.read_word_fast(69).as_i64() as i32;

            // 2. Executa a sequência exata de instruções emitida por `clang --target=riscv32 -march=rv32im -O2`
            // para `R_PointOnSide` com `FixedMul` inline (`mul + mulh + slli 16 + srli 16 + or`):
            rv.write_reg(10, x16); // a0 = x
            rv.write_reg(11, y16); // a1 = y
            rv.write_reg(18, ldx16); // lw s2, 8(a2) (node->dx)
            rv.step_branch_or_store(); // beqz s2, .LBB1_3
            rv.write_reg(13, ldy16); // lw a3, 12(a2) (node->dy)
            rv.step_branch_or_store(); // beqz a3, .LBB1_5
            rv.write_reg(14, lx16); // lw a4, 0(a2) (node->x)
            rv.write_reg(15, ly16); // lw a5, 4(a2) (node->y)
            let dx = x16.wrapping_sub(lx16);
            let dy = y16.wrapping_sub(ly16);
            rv.write_reg(12, dx); // sub a2, a0, a4
            rv.write_reg(8, dy); // sub s0, a1, a5

            // FixedMul(node->dy >> 16, dx) em RV32IM:
            let ldy_s = ldy16 >> 16;
            rv.write_reg(10, ldy_s); // srai a0, a3, 16
            let prod_left = (ldy_s as i64).wrapping_mul(dx as i64);
            let left_lo = prod_left as i32;
            let left_hi = (prod_left >> 32) as i32;
            rv.write_reg(16, left_lo); // mul a6, a0, a2
            rv.write_reg(17, left_hi); // mulh a7, a0, a2
            rv.write_reg(17, left_hi << 16); // slli a7, a7, 16
            rv.write_reg(16, ((left_lo as u32) >> 16) as i32); // srli a6, a6, 16
            let left = (left_hi << 16) | (((left_lo as u32) >> 16) as i32);
            rv.write_reg(9, left); // or s1, a6, a7

            // FixedMul(dy, node->dx >> 16) em RV32IM:
            let ldx_s = ldx16 >> 16;
            rv.write_reg(11, ldx_s); // srai a1, s2, 16
            let prod_right = (dy as i64).wrapping_mul(ldx_s as i64);
            let right_lo = prod_right as i32;
            let right_hi = (prod_right >> 32) as i32;
            rv.write_reg(16, right_lo); // mul a6, s0, a1
            rv.write_reg(17, right_hi); // mulh a7, s0, a1
            rv.write_reg(17, right_hi << 16); // slli a7, a7, 16
            rv.write_reg(16, ((right_lo as u32) >> 16) as i32); // srli a6, a6, 16
            let right = (right_hi << 16) | (((right_lo as u32) >> 16) as i32);
            rv.write_reg(10, right); // or a0, a6, a7

            let slt = if right < left { 1 } else { 0 };
            rv.write_reg(10, slt); // slt a0, a0, s1
            let side_rv32 = slt ^ 1;
            rv.write_reg(10, side_rv32); // xori a0, a0, 1
            rv.step_branch_or_store(); // ret

            // Garante que o resultado geométrico ternário e binário concordam exatamente!
            assert_eq!(side_ternary, side_rv32);
        }
        let elapsed_us = t0.elapsed().as_micros().max(1);

        let mut tel = machine.cpu.telemetry;
        tel.binary32_bit_flips = rv.bit_flips;

        IsaBenchmarkResult {
            name: "3. BSP R_PointOnSide (ROM)",
            ternary_instructions: machine.cpu.cycles,
            binary_rv32i_instructions: rv.instructions,
            telemetry: tel,
            elapsed_us,
        }
    }

    /// 4. Benchmark do Loop Interno `R_DrawColumn` no Framebuffer VGA (`320x200`).
    /// Compara `TernaryFixed` (`11.10`, escala $3^{10}$) contra `RV32I` (`16.16`, escala $2^{16}$).
    fn bench_r_draw_column_kernel() -> IsaBenchmarkResult {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        let step = TernaryFixed::from_f64(0.85);
        mem.write_word_fast(600, step.raw_word());

        let asm = r#"
            LOAD_IX_IMM IX0, 999        ; desenha 1000 texels verticais no Framebuffer VGA
            LOAD_IX_IMM IX2, 0          ; IX2 = frac (mantido em registrador como no RV32I!)
        .col_loop:
            ADD_IX IX2, 600             ; IX2 += fracstep (11.10 trits no registrador IX2)
            SHF_IX_ACC IX2, -10         ; ACC = IX2 >> 10 trits (sem destruir IX2!)
            STORE_INDEX IX0, 4600000    ; grava Tryte6 no Framebuffer VGA
            BIX IX0, .col_loop          ; IX0 -= 1; se IX0 >= 0 salta sem poluir ACC!
            HALT
        "#;

        let prog = SetunAssembler::assemble(asm).unwrap();
        cpu.load_program(&mut mem, 0, &prog);

        let t0 = Instant::now();
        cpu.run_ticks(&mut mem, 200_000);
        let elapsed_us = t0.elapsed().as_micros().max(1);
        assert!(cpu.halted && cpu.trap.is_none());

        // Execução real em RV32I com 32 registradores usando fracstep 16.16 nativo (`0.85 * 65536 = 55706`):
        // Observe que em RV32I `frac` permanece no registrador `x3` sem precisar de LOAD/STORE a cada iteração!
        let mut rv = Rv32iReferenceCpu::new();
        let step16 = (0.85f64 * 65536.0).round() as i32;
        rv.write_reg(1, 999); // li x1, 999
        rv.write_reg(2, step16); // li x2, 55706
        rv.write_reg(3, 0); // li x3, 0 (frac)
        rv.write_reg(4, 4_600_000); // lui + addi x4, 4600000 (2 instrs)
        rv.step_branch_or_store();
        for _ in 0..1000 {
            let next_frac = rv.regs[3].wrapping_add(rv.regs[2]);
            rv.write_reg(3, next_frac); // add x3, x3, x2
            let tex_idx = next_frac >> 16;
            rv.write_reg(5, tex_idx); // srai x5, x3, 16
            rv.step_branch_or_store(); // sb x5, 0(x4)
            let next_i = rv.regs[1] - 1;
            rv.write_reg(1, next_i); // addi x1, x1, -1
            rv.step_branch_or_store(); // bge x1, x0, .col_loop
        }
        rv.step_branch_or_store(); // ret

        let mut tel = cpu.telemetry;
        tel.binary32_bit_flips = rv.bit_flips;

        IsaBenchmarkResult {
            name: "4. Doom R_DrawColumn Kernel",
            ternary_instructions: cpu.cycles,
            binary_rv32i_instructions: rv.instructions,
            telemetry: tel,
            elapsed_us,
        }
    }

    /// 5. Stress Test de Alta Cadência (~2,4 Milhões de Instruções, mesma escala do Dhrystone do paper).
    fn bench_dhrystone_stress_2m_cycles() -> IsaBenchmarkResult {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        mem.write_word_fast(700, Word21::new(3));
        mem.write_word_fast(701, Word21::new(-3));

        let asm = r#"
            LOAD_IX_IMM IX0, 299999
            LOAD_IMM 1
        .dhry_loop:
            ADD 700
            STI.T
            SUB 701
            CMP.T 700
            BIX IX0, .dhry_loop
            HALT
        "#;

        let prog = SetunAssembler::assemble(asm).unwrap();
        cpu.load_program(&mut mem, 0, &prog);

        let t0 = Instant::now();
        cpu.run_ticks(&mut mem, 3_000_000);
        let elapsed_us = t0.elapsed().as_micros().max(1);
        assert!(cpu.halted && cpu.trap.is_none());

        // Execução real em RV32I de 32 registradores (300.000 iterações):
        let mut rv = Rv32iReferenceCpu::new();
        rv.write_reg(1, 299_999); // li x1, 299999
        rv.write_reg(2, 3); // li x2, 3
        rv.write_reg(3, -3); // li x3, -3
        rv.write_reg(4, 1); // li x4, 1 (acc)
        for _ in 0..300_000 {
            let s1 = rv.regs[4].wrapping_add(rv.regs[2]);
            rv.write_reg(4, s1); // add x4, x4, x2
            let s2 = (-s1).wrapping_sub(rv.regs[3]);
            rv.write_reg(4, -s1); // neg x4, x4
            rv.write_reg(4, s2); // sub x4, x4, x3
            // CMP.T (x4 <=> x2) em RV32I: slt + slt + sub
            let gt = if s2 > rv.regs[2] { 1 } else { 0 };
            let lt = if s2 < rv.regs[2] { 1 } else { 0 };
            rv.write_reg(5, gt);
            rv.write_reg(6, lt);
            rv.write_reg(4, gt - lt);
            let next_i = rv.regs[1] - 1;
            rv.write_reg(1, next_i); // addi x1, x1, -1
            rv.step_branch_or_store(); // bge x1, x0, .dhry_loop
        }
        rv.step_branch_or_store(); // ret

        let mut tel = cpu.telemetry;
        tel.binary32_bit_flips = rv.bit_flips;

        IsaBenchmarkResult {
            name: "5. Stress 2.4M (Escala Dhrystone)",
            ternary_instructions: cpu.cycles,
            binary_rv32i_instructions: rv.instructions,
            telemetry: tel,
            elapsed_us,
        }
    }

    /// Compara a precisão e a simetria de arredondamento de `TernaryFixed` (`11.10`) contra
    /// `fixed_t` binário (`16.16` do Doom).
    pub fn bench_fixed_point_precision() -> FixedPrecisionReport {
        let iterations = 1_000;
        let mut sum_sq_err_ternary = 0.0f64;
        let mut sum_sq_err_binary = 0.0f64;
        let mut ternary_sym_err = 0.0f64;
        let mut binary_asym_bias = 0.0f64;

        for i in 1..=iterations {
            let val_f64 = (i as f64) * 0.13729 - 68.5;

            // Ponto fixo ternário 11.10: truncamento balanceado de 10 trits (arredonda para o inteiro mais próximo)
            let tf_pos = TernaryFixed::from_f64(val_f64);
            let tf_neg = TernaryFixed::from_f64(-val_f64);
            let t_int_pos = tf_pos.to_int_balanced();
            let t_int_neg = tf_neg.to_int_balanced();
            let nearest_int = val_f64.round();

            sum_sq_err_ternary += ((t_int_pos as f64) - nearest_int).powi(2);
            ternary_sym_err += (t_int_pos + t_int_neg).abs() as f64;

            // Ponto fixo binário 16.16 do Doom: shift aritmético `>> 16` trunca sempre para -infinito!
            let bf_pos = (val_f64 * (DOOM_BINARY_FRACUNIT as f64)).round() as i32;
            let bf_neg = (-val_f64 * (DOOM_BINARY_FRACUNIT as f64)).round() as i32;
            let b_int_pos = bf_pos >> 16;
            let b_int_neg = bf_neg >> 16;

            sum_sq_err_binary += ((b_int_pos as f64) - nearest_int).powi(2);
            // Em complemento de dois, `(x >> 16) + ((-x) >> 16)` dá -1 para quase todo x fracionário!
            binary_asym_bias += (b_int_pos + b_int_neg).abs() as f64;
        }

        FixedPrecisionReport {
            iterations,
            ternary_rmse: (sum_sq_err_ternary / (iterations as f64)).sqrt(),
            binary16_rmse: (sum_sq_err_binary / (iterations as f64)).sqrt(),
            ternary_sign_symmetry_error: ternary_sym_err / (iterations as f64),
            binary16_sign_asymmetry_bias: binary_asym_bias / (iterations as f64),
        }
    }

    /// Mede a taxa de quadros por segundo (FPS) do pipeline 3D completo:
    /// Travessia BSP Tricótoma + Clipper de Oclusão em 16 palavras `Word21` +
    /// `R_DrawColumn` (`320x200`) + Tradução `Tryte6 -> RGBA32` (`DG_DrawFrame`).
    pub fn bench_bsp_and_vga_throughput(frames: usize) -> RenderThroughputReport {
        let mut mem = Memory::new();
        let mut rgba = vec![0u8; FB_SIZE * 4];

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

        let t0 = Instant::now();
        for f in 0..frames {
            let cam_x = TernaryFixed::from_int(2 + ((f % 5) as i64));
            TernaryRenderer::render_bsp_scene(&mut mem, &bsp_tree, cam_x, TernaryFixed::ZERO);
            mem.render_framebuffer_rgba(&mut rgba);
        }
        let elapsed_s = t0.elapsed().as_secs_f64().max(1e-9);
        let fps = (frames as f64) / elapsed_s;
        let megapixels_per_sec = ((frames * FB_SIZE) as f64 / 1_000_000.0) / elapsed_s;

        RenderThroughputReport {
            frames,
            elapsed_ms: elapsed_s * 1000.0,
            fps,
            megapixels_per_sec,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_suite_sanity() {
        let b1 = BenchmarkRunner::bench_three_way_search_bceg();
        assert!(b1.instruction_diff_pct() < 0.0);
        assert!(b1.digit_flip_diff_pct() < 0.0);
        assert!(b1.weighted_energy_diff_pct() < -40.0);

        let b3 = BenchmarkRunner::bench_bsp_zero_crossing_determinant();
        assert!(b3.digit_flip_diff_pct() < -30.0);

        let prec = BenchmarkRunner::bench_fixed_point_precision();
        // Simetria de sinal no ternário deve ser zero absoluto (0.0), enquanto no binário >> 16 há viés (~1.0)
        assert_eq!(prec.ternary_sign_symmetry_error, 0.0);
        assert!(prec.binary16_sign_asymmetry_bias > 0.9);
    }
}
