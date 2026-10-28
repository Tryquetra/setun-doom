//! # Scientific and Graphics Engine Benchmark Suite (`Setun-Doom` vs. Binary `RV32I`)
//!
//! Implements comparative experiments between the balanced ternary architecture (`Setun-Doom` /
//! `REBEL-6`) and the binary 32-bit two's complement reference (`RV32I`):
//!
//! 1. **Workload Comparison Table (IEEE ISMVL 2025 Table IV Style)**:
//!    - **3-Way Search with `BCEG`** (*Three-Way Search* on sorted array in ternary memory).
//!    - **Dijkstra 3-Way Quicksort Partitioning** (*Dutch National Flag* using `CMP.T` and `BCEG`).
//!    - **Sign Oscillation & BSP Plane Traversal** (cross product crossing zero repeatedly).
//!    - **Column Interpolator `R_DrawColumn`** (texture coordinate accumulation into VGA Framebuffer).
//!    - **Ternary Micro-Dhrystone** (intensive workload of millions of cycles to measure real host MIPS).
//! 2. **Numerical Precision Analysis (Drift / RMSE)**:
//!    - Compares accumulated error of ternary `11.10` fixed-point ([`TernaryFixed`], symmetric rounding)
//!      against Doom's binary `16.16` `fixed_t` (asymmetric truncation toward $-\infty$) relative to `f64`.
//! 3. **3D Rendering Throughput (BSP + Occlusion Clipper + RGBA)**:
//!    - Measures equivalent frames per second (FPS) and millions of pixels/second through the full pipeline.

use crate::assembler::SetunAssembler;
use crate::bsp::{BspChild, BspNode, BspTree, Subsector, WallSeg};
use crate::cpu::{Cpu, PowerTelemetry};
use crate::fixed::{TernaryFixed, DOOM_BINARY_FRACUNIT};
use crate::memory::{Memory, FB_SIZE};
use crate::render::TernaryRenderer;
use crate::word::{Tryte6, Word21};
use std::time::Instant;

/// Consolidated result of an individual ISA benchmark (IEEE ISMVL 2025 Table IV Style).
#[derive(Clone, Debug)]
pub struct IsaBenchmarkResult {
    /// Benchmark name.
    pub name: &'static str,
    /// Instructions executed on ternary ISA (`Setun-Doom`).
    pub ternary_instructions: u64,
    /// Equivalent instructions required in binary `RV32I` (where each `BCEG` requires `blt + beq`
    /// and 21-trit constants require `lui + addi`).
    pub binary_rv32i_instructions: u64,
    /// State transition telemetry (*Trit Flips*, *Half-Swings*, *Full-Swings*, and *Bit Flips*).
    pub telemetry: PowerTelemetry,
    /// Wall-clock time spent in simulation in microseconds.
    pub elapsed_us: u128,
}

impl IsaBenchmarkResult {
    /// Percentage difference of executed instructions (`< 0.0` indicates fewer instructions in ternary).
    pub fn instruction_diff_pct(&self) -> f64 {
        if self.binary_rv32i_instructions == 0 {
            return 0.0;
        }
        let b = self.binary_rv32i_instructions as f64;
        let t = self.ternary_instructions as f64;
        ((t - b) / b) * 100.0
    }

    /// Percentage difference of *Digit Flips* (`< 0.0` indicates fewer transitions in ternary).
    pub fn digit_flip_diff_pct(&self) -> f64 {
        -self.telemetry.digit_flip_reduction_pct()
    }

    /// Percentage difference of weighted capacitive energy ($\Delta V^2$, where *Half-Swing* = 0.25x).
    pub fn weighted_energy_diff_pct(&self) -> f64 {
        -self.telemetry.weighted_energy_reduction_pct()
    }

    /// Millions of ternary instructions executed per second (MIPS) on host Rust simulator.
    pub fn mips(&self) -> f64 {
        if self.elapsed_us == 0 {
            return 0.0;
        }
        (self.ternary_instructions as f64) / (self.elapsed_us as f64)
    }
}

/// Numerical precision experiment result (`TernaryFixed 11.10` vs. `Doom Binary 16.16`).
#[derive(Copy, Clone, Debug)]
pub struct FixedPrecisionReport {
    pub iterations: usize,
    pub ternary_rmse: f64,
    pub binary16_rmse: f64,
    pub ternary_sign_symmetry_error: f64,
    pub binary16_sign_asymmetry_bias: f64,
}

/// 3D rendering throughput and RGBA conversion performance result.
#[derive(Copy, Clone, Debug)]
pub struct RenderThroughputReport {
    pub frames: usize,
    pub elapsed_ms: f64,
    pub fps: f64,
    pub megapixels_per_sec: f64,
}

/// Global report for the Setun-Doom benchmark suite.
pub struct FullBenchmarkSuiteReport {
    pub isa_benchmarks: Vec<IsaBenchmarkResult>,
    pub fixed_precision: FixedPrecisionReport,
    pub render_throughput: RenderThroughputReport,
}

/// Real `RV32I` reference executor with 32-register file (`x0..x31` in two's complement).
/// Eliminates static estimates (`cpu.cycles + N`): each benchmark executes the equivalent algorithm
/// directly across the 32 `RV32I` registers (without accumulator penalty `MOV_IX_ACC`),
/// tracking actual instructions and exact *Bit Flips* across `x0..x31`.
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

    /// Writes to `regs[rd]` (with `x0` tied to 0) and adds Hamming distance `(prev ^ next).count_ones()`.
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

    /// Records an instruction that does not modify registers (e.g., `beq`, `blt`, `bge`, `j`, `sw`).
    #[inline(always)]
    pub fn step_branch_or_store(&mut self) {
        self.instructions += 1;
    }
}

/// Test runner for scientific and graphics engine benchmarks.
pub struct BenchmarkRunner;

impl BenchmarkRunner {
    /// Executes all benchmarks and returns the consolidated report.
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

    /// 1. 3-Way Sorted Search Benchmark using `BCEG` (`Three-Way Search`).
    /// Compares actual execution on `Setun-Doom` CPU against actual execution on 32-register `RV32I`.
    fn bench_three_way_search_bceg() -> IsaBenchmarkResult {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        for i in 0..256 {
            let val = ((i as i64) - 128) * 10;
            mem.write_word_fast(1000 + (i as i64), Word21::new(val));
        }

        let asm = r#"
            LOAD_IX_IMM IX0, 0          ; current index in array (0..255)
            LOAD_IX_IMM IX1, 700        ; target key (+700 -> index 198)
            LOAD_IMM 0
            STORE 900                   ; step counter
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
            STORE 901                   ; store found index (198)
            HALT
        "#;

        let prog = SetunAssembler::assemble(asm).unwrap();
        cpu.load_program(&mut mem, 0, &prog);

        let t0 = Instant::now();
        cpu.run_ticks(&mut mem, 50_000);
        let elapsed_us = t0.elapsed().as_micros().max(1);

        assert!(cpu.halted && cpu.trap.is_none());
        assert_eq!(mem.read_word_fast(901).as_i64(), 198);

        // Executes the same algorithm on the 32-register RV32I processor:
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
            name: "1. 3-Way Search (BCEG Search)",
            ternary_instructions: cpu.cycles,
            binary_rv32i_instructions: rv.instructions,
            telemetry: tel,
            elapsed_us,
        }
    }

    /// 2. Dijkstra 3-Way Partitioning Benchmark (*Dutch National Flag* / Ternary Quicksort).
    /// Compares ternary accumulator + `BCEG` against 32-register `RV32I` (where `RV32I` does not pay `MOV_IX_ACC`).
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
            LOAD_IX_IMM IX0, 299        ; sweep cursor (299..0)
            LOAD_IX_IMM IX1, 0          ; pivot = 0
            LOAD_IX_IMM IX2, 0          ; count < 0
            LOAD_IX_IMM IX3, 0          ; count == 0
            LOAD_IX_IMM IX4, 0          ; count > 0
        .part_loop:
            LOAD_INDEX IX0, 2000        ; ACC = array[IX0]
            BCEG IX1, .is_zero, .is_pos
            ADD_IX_IMM IX2, 1
            JMP .next_elem
        .is_zero:
            ADD_IX_IMM IX3, 1
            JMP .next_elem
        .is_pos:
            ADD_IX_IMM IX4, 1
        .next_elem:
            BIX IX0, .part_loop         ; IX0 -= 1; if IX0 >= 0 branch without destroying ACC
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

        // Actual execution on RV32I with 32 registers (x1=i, x2=neg, x3=zero, x4=pos, x5=elem):
        // Note that on RV32I the loop end uses `addi x1, x1, -1; bge x1, x0, .part_loop` without needing MOV_IX_ACC.
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

    /// 3. Real BSP Plane Traversal Benchmark (Doom's `ROM_POINT_ON_SIDE`).
    /// Executes the firmware subroutine `ROM_POINT_ON_SIDE_ENTRY` (full `R_PointOnSide` with
    /// `SUB`, `SHIFT -10`, `MUL`, `CMP_T`, `JMP_N`) for 200 points oscillating between front and back
    /// of a BSP partition line, compared against `R_PointOnSide` in `RV32I` (`16.16` binary).
    fn bench_bsp_zero_crossing_determinant() -> IsaBenchmarkResult {
        let mut machine = crate::mmo::SetunDoomMachine::new();
        let mut rv = Rv32iReferenceCpu::new();

        // Diagonal partition line: lx = 100.0, ly = -50.0, ldx = 24.0, ldy = -18.0
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

        // Initialize constant registers in RV32I: x10=lx, x11=ly, x12=ldx, x13=ldy
        rv.write_reg(10, lx16);
        rv.write_reg(11, ly16);
        rv.write_reg(12, ldx16);
        rv.write_reg(13, ldy16);

        let t0 = Instant::now();
        for i in 0..200i32 {
            // Oscillate point (x, y) across the partition line on every iteration
            let sign = if i % 2 == 0 { 1 } else { -1 };
            let x16 = (100 + sign * ((i % 37) + 1)) * 65536 + (sign * 16384);
            let y16 = (-50 - sign * ((i % 29) + 1)) * 65536 - (sign * 8192);

            // 1. Execute ROM_POINT_ON_SIDE_ENTRY on Setun-Doom CPU
            machine
                .memory
                .write_word_fast(60, TernaryFixed::from_doom_fixed16(x16).raw_word());
            machine
                .memory
                .write_word_fast(61, TernaryFixed::from_doom_fixed16(y16).raw_word());
            machine.execute_rom_routine(crate::mmo::ROM_POINT_ON_SIDE_ENTRY, 32);
            let side_ternary = machine.memory.read_word_fast(69).as_i64() as i32;

            // 2. Execute exact instruction sequence emitted by `clang --target=riscv32 -march=rv32im -O2`
            // for `R_PointOnSide` with inline `FixedMul` (`mul + mulh + slli 16 + srli 16 + or`):
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

            // FixedMul(node->dy >> 16, dx) in RV32IM:
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

            // FixedMul(dy, node->dx >> 16) in RV32IM:
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

            // Ensure ternary and binary geometric results match exactly
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

    /// 4. Benchmark of Inner Loop `R_DrawColumn` in VGA Framebuffer (`320x200`).
    /// Compares `TernaryFixed` (`11.10`, scale $3^{10}$) against `RV32I` (`16.16`, scale $2^{16}$).
    fn bench_r_draw_column_kernel() -> IsaBenchmarkResult {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        let step = TernaryFixed::from_f64(0.85);
        mem.write_word_fast(600, step.raw_word());

        let asm = r#"
            LOAD_IX_IMM IX0, 999        ; render 1000 vertical texels in VGA Framebuffer
            LOAD_IX_IMM IX2, 0          ; IX2 = frac (held in register as in RV32I)
        .col_loop:
            ADD_IX IX2, 600             ; IX2 += fracstep (11.10 trits in register IX2)
            SHF_IX_ACC IX2, -10         ; ACC = IX2 >> 10 trits (without destroying IX2)
            STORE_INDEX IX0, 4600000    ; write Tryte6 to VGA Framebuffer
            BIX IX0, .col_loop          ; IX0 -= 1; if IX0 >= 0 branch without polluting ACC
            HALT
        "#;

        let prog = SetunAssembler::assemble(asm).unwrap();
        cpu.load_program(&mut mem, 0, &prog);

        let t0 = Instant::now();
        cpu.run_ticks(&mut mem, 200_000);
        let elapsed_us = t0.elapsed().as_micros().max(1);
        assert!(cpu.halted && cpu.trap.is_none());

        // Actual execution on RV32I with 32 registers using native 16.16 fracstep (`0.85 * 65536 = 55706`):
        // Note that on RV32I `frac` remains in register `x3` without needing LOAD/STORE each iteration.
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

    /// 5. High-Cadence Stress Test (~2.4 Million Instructions, same scale as paper's Dhrystone).
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

        // Actual execution on 32-register RV32I (300,000 iterations):
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
                                 // CMP.T (x4 <=> x2) in RV32I: slt + slt + sub
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
            name: "5. Stress 2.4M (Dhrystone Scale)",
            ternary_instructions: cpu.cycles,
            binary_rv32i_instructions: rv.instructions,
            telemetry: tel,
            elapsed_us,
        }
    }

    /// Compares numerical precision and rounding symmetry of `TernaryFixed` (`11.10`) against
    /// binary `fixed_t` (`16.16` from Doom).
    pub fn bench_fixed_point_precision() -> FixedPrecisionReport {
        let iterations = 1_000;
        let mut sum_sq_err_ternary = 0.0f64;
        let mut sum_sq_err_binary = 0.0f64;
        let mut ternary_sym_err = 0.0f64;
        let mut binary_asym_bias = 0.0f64;

        for i in 1..=iterations {
            let val_f64 = (i as f64) * 0.13729 - 68.5;

            // Ternary fixed-point 11.10: balanced truncation of 10 trits (rounds to nearest integer)
            let tf_pos = TernaryFixed::from_f64(val_f64);
            let tf_neg = TernaryFixed::from_f64(-val_f64);
            let t_int_pos = tf_pos.to_int_balanced();
            let t_int_neg = tf_neg.to_int_balanced();
            let nearest_int = val_f64.round();

            sum_sq_err_ternary += ((t_int_pos as f64) - nearest_int).powi(2);
            ternary_sym_err += (t_int_pos + t_int_neg).abs() as f64;

            // Doom binary fixed-point 16.16: arithmetic shift `>> 16` always truncates toward -infinity
            let bf_pos = (val_f64 * (DOOM_BINARY_FRACUNIT as f64)).round() as i32;
            let bf_neg = (-val_f64 * (DOOM_BINARY_FRACUNIT as f64)).round() as i32;
            let b_int_pos = bf_pos >> 16;
            let b_int_neg = bf_neg >> 16;

            sum_sq_err_binary += ((b_int_pos as f64) - nearest_int).powi(2);
            // In two's complement, `(x >> 16) + ((-x) >> 16)` produces -1 for nearly all fractional x
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

    /// Measures frames per second (FPS) across the full 3D pipeline:
    /// Trichotomous BSP Traversal + Occlusion Clipper across 16 `Word21` words +
    /// `R_DrawColumn` (`320x200`) + Translation `Tryte6 -> RGBA32` (`DG_DrawFrame`).
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
        // Sign symmetry in ternary must be absolute zero (0.0), whereas in binary >> 16 there is bias (~1.0)
        assert_eq!(prec.ternary_sign_symmetry_error, 0.0);
        assert!(prec.binary16_sign_asymmetry_bias > 0.9);
    }
}
