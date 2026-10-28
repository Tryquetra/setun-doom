// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(let_underscore_drop, clippy::let_underscore_must_use)]

//! # Setun-Doom Architecture Emulator
//!
//! Rust emulator for a balanced ternary computer architecture inspired by
//! **Setun** (1958, Nikolai Brusentsov), scaled to 21/27 trits (`Setun-Doom ISA`)
//! to execute the classic **Doom (1993)** engine via `doomgeneric`.
//!
//! ## Module Organization
//! - [`trit`]: `Trit` representation (`#[repr(i8)]` in `{-1, 0, +1}`), Kleene/Lukasiewicz logic
//!   gates, symmetric wire inversion, and full ternary adder.
//! - [`word`]: Long words `Word21` ($3^{21} \approx 1.046 \times 10^{10}$ states) and
//!   `Word27`, 6-trit `Tryte6` (`[-364, +364]`), and dual-bit encoding.
//! - [`fixed`]: Balanced ternary fixed-point arithmetic `TernaryFixed` (`11.10` trits,
//!   scale factor $3^{10} = 59,049$) replacing Doom's `16.16` `fixed_t`.
//! - [`memory`]: Linear memory space of $3^{14} = 4,782,969$ words and VGA Framebuffer
//!   mapping (`320x200 = 64,000` pixels in `Tryte6`).
//! - [`cpu`]: Machine state (`ACC`, `R`, `PC`, `SP`, 9 index registers `IX`), 21-trit ISA,
//!   and Fetch-Decode-Execute cycle with trichotomous branches (`JMPN`, `JMPZ`, `JMPP`).
//! - [`mmo`]: DMA Controller / binary WAD file loader (`DOOM.WAD`) and C FFI bindings
//!   for the 5 `doomgeneric` platform callbacks (`DG_Init`, `DG_DrawFrame`, `DG_SleepMs`,
//!   `DG_GetTicksMs`, `DG_GetKey`).
//! - [`bsp`]: Trichotomous BSP tree (`R_PointOnSide`, `R_RenderBSPNode`) and horizontal occlusion
//!   clipper across a trit mesh (`solidsegs` in 16 `Word21` words).
//! - [`assembler`]: Two-pass text assembler (`SetunAssembler`) with label support,
//!   `IX0..IX8` registers, `0t+0-` ternary literals, and reverse disassembler.
//! - [`render`]: 3D column rasterizer (`R_DrawColumn`), floor rasterizer (`R_DrawSpan`), and BSP scene renderer.

#[cfg(not(feature = "std"))]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
pub mod assembler;
#[cfg(feature = "std")]
pub mod benchmarks;
pub mod bsp;
#[cfg(feature = "std")]
pub mod codegen;
pub mod cpu;
#[cfg(feature = "std")]
pub mod dbt;
pub mod dos;
#[cfg(feature = "std")]
pub mod elf;
pub mod fixed;
#[cfg(feature = "std")]
pub mod gui;
pub mod memory;
#[cfg(feature = "std")]
pub mod mmo;
pub mod render;
pub mod stack_machine;
pub mod trit;
pub mod word;

#[cfg(feature = "std")]
pub use assembler::{AsmError, SetunAssembler};
#[cfg(feature = "std")]
pub use benchmarks::{
    BenchmarkRunner, FixedPrecisionReport, FullBenchmarkSuiteReport, IsaBenchmarkResult,
    RenderThroughputReport,
};
pub use bsp::{BspChild, BspNode, BspTree, Subsector, TernaryOcclusionClipper, WallSeg};
pub use cpu::{Cpu, Instruction, Opcode, PowerTelemetry};
#[cfg(feature = "std")]
pub use dbt::{
    run_all_riscv_isa_tests, DbtError, DbtExecutionReport, DbtExitReason, RiscvIsaSuiteSummary,
    Rv32Platform, Rv32iInstr, SetunDbtEngine, SetunTranslatedOp, RISCV_ISA_ELF_TESTS,
};
#[cfg(feature = "std")]
pub use dos::doom_runner::DoomDosRunner;
pub use dos::{tfx::TfxExecutable, ActiveWindow, SetunDesktop, SetunDos};
#[cfg(feature = "std")]
pub use elf::{Elf32Image, ElfError, ElfSection, ElfSegment, LoadedElfLayout};
pub use fixed::{TernaryFixed, FRAC_SCALE, FRAC_TRITS, INT_TRITS};
#[cfg(feature = "std")]
pub use gui::BenchGui;
pub use memory::{Memory, MemoryError, FB_HEIGHT, FB_SIZE, FB_WIDTH, FRAMEBUFFER_BASE, MEMORY_SIZE};
#[cfg(feature = "std")]
pub use mmo::{
    DG_DrawFrame, DG_DrawFrameToBuffer, DG_GetKey, DG_GetTicksMs, DG_Init, DG_SleepMs,
    DmaTranscodeMode, KeyEvent, SetunDoomMachine, SetunHookCounters, WadDmaLoader, WadLumpInfo,
    global_machine, lock_machine, setun_GetHookCounters,
};
pub use render::{ColumnDrawParams, TernaryRenderer};
pub use stack_machine::{StackError, StackMachine, StackOp, StackProgram};
pub use trit::Trit;
pub use word::{Tryte6, Word21, Word27};
