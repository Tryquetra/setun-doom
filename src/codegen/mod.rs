//! Setun-Doom Native Code Generation Subsystem from LLVM IR (Level 2).

pub mod ir;
pub mod isel;
pub mod linker;
pub mod parser;

pub use ir::{LlvmBlock, LlvmFunction, LlvmInst, LlvmModule, LlvmValue};
pub use isel::SetunCodeGenerator;
pub use linker::{LinkedImage, RelocationRecord, SetunLinker, CODE_SEGMENT_BASE, RODATA_SEGMENT_BASE};
pub use parser::LlvmParser;
