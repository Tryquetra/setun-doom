//! REBEL-6 core: 32-trit instruction encoding, 24-trit word arithmetic, 729-register file,
//! byte/tryte addressed data memory, and a Harvard execution core.

#[cfg(feature = "std")]
pub mod asm;
pub mod cpu;
pub mod encoding;
pub mod mem;
pub mod word;

#[cfg(feature = "std")]
pub use asm::{assemble, AsmError, Image, DATA_BASE};
pub use cpu::{Cpu, Outcome, RunResult, Trap, REGISTERS};
pub use encoding::{decode, encode, get_field, get_trit, Format, Instr, Op};
pub use mem::{DataMem, MemError};
pub use word::{Word24, HALF, MODULUS, TRITS};
