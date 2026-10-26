//! # Setun-Doom Architecture Emulator
//!
//! Emulador em Rust de uma arquitetura de computadores ternária balanceada inspirada no
//! **Setun** (1958, Nikolai Brusentsov), escalada para 21/27 trits (`Setun-Doom ISA`)
//! para viabilizar a execução do motor clássico do **Doom (1993)** via `doomgeneric`.
//!
//! ## Organização Modular
//! - [`trit`]: Representação de `Trit` (`#[repr(i8)]` em `{-1, 0, +1}`), portas lógicas de
//!   Kleene/Lukasiewicz, inversão simétrica e somador completo ternário.
//! - [`word`]: Palavras longas `Word21` ($3^{21} \approx 1{,}046 \times 10^{10}$ estados) e
//!   `Word27`, `Tryte6` de 6 trits (`[-364, +364]`) e codificação dual-bit.
//! - [`fixed`]: Aritmética de ponto fixo ternário balanceado `TernaryFixed` (`11.10` trits,
//!   fator de escala $3^{10} = 59.049$) substituindo o `fixed_t` `16.16` do Doom.
//! - [`memory`]: Espaço de memória linear de $3^{14} = 4.782.969$ palavras e mapeamento de
//!   Framebuffer VGA (`320x200 = 64.000` pixels em `Tryte6`).
//! - [`cpu`]: Estado da máquina (`ACC`, `R`, `PC`, `SP`, 9 registradores de índice `IX`),
//!   ISA de 21 trits e ciclo Fetch-Decode-Execute com desvios tricótomos (`JMPN`, `JMPZ`, `JMPP`).
//! - [`mmo`]: Controlador DMA/Loader de arquivos binários (`DOOM.WAD`) e bindings FFI C
//!   para as 5 rotinas de plataforma do `doomgeneric` (`DG_Init`, `DG_DrawFrame`, `DG_SleepMs`,
//!   `DG_GetTicksMs`, `DG_GetKey`).
//! - [`bsp`]: Árvore BSP Tricótoma (`R_PointOnSide`, `R_RenderBSPNode`) e Clipper Horizontal
//!   de Oclusão em malha de trits (`solidsegs` em 16 palavras `Word21`).
//! - [`assembler`]: Montador textual de duas passagens (`SetunAssembler`) com suporte a labels,
//!   registradores `IX0..IX8`, literais ternários `0t+0-` e disassembler reverso.
//! - [`render`]: Rasterizador 3D de colunas (`R_DrawColumn`), pisos (`R_DrawSpan`) e cena BSP.

pub mod assembler;
pub mod benchmarks;
pub mod bsp;
pub mod cpu;
pub mod dbt;
pub mod elf;
pub mod fixed;
pub mod memory;
pub mod mmo;
pub mod render;
pub mod trit;
pub mod word;

pub use assembler::{AsmError, SetunAssembler};
pub use benchmarks::{
    BenchmarkRunner, FixedPrecisionReport, FullBenchmarkSuiteReport, IsaBenchmarkResult,
    RenderThroughputReport,
};
pub use bsp::{BspChild, BspNode, BspTree, Subsector, TernaryOcclusionClipper, WallSeg};
pub use cpu::{Cpu, Instruction, Opcode, PowerTelemetry};
pub use dbt::{
    run_all_riscv_isa_tests, DbtError, DbtExecutionReport, DbtExitReason, RiscvIsaSuiteSummary,
    Rv32iInstr, SetunDbtEngine, SetunTranslatedOp, RISCV_ISA_ELF_TESTS,
};
pub use elf::{Elf32Image, ElfError, ElfSection, ElfSegment, LoadedElfLayout};
pub use fixed::{TernaryFixed, FRAC_SCALE, FRAC_TRITS, INT_TRITS};
pub use memory::{Memory, MemoryError, FB_HEIGHT, FB_SIZE, FB_WIDTH, FRAMEBUFFER_BASE, MEMORY_SIZE};
pub use mmo::{
    DG_DrawFrame, DG_DrawFrameToBuffer, DG_GetKey, DG_GetTicksMs, DG_Init, DG_SleepMs,
    DmaTranscodeMode, KeyEvent, SetunDoomMachine, WadDmaLoader, WadLumpInfo, global_machine,
};
pub use render::{ColumnDrawParams, TernaryRenderer};
pub use trit::Trit;
pub use word::{Tryte6, Word21, Word27};
