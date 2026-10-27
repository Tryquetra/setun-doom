// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Dynamic Binary Translator (`DBT / JIT`) from `RV32I` (32 bits) to `Setun-Doom` (`21 trits`).
//!
//! Implements **Level 1 Ternary Emulation**:
//! 1. **Complete decoder for base `RV32I` ISA** (all 37 user and system instructions).
//! 2. **Register file of 32 `x0..x31` registers in [`Word21`]** mapped onto Setun linear memory
//!    (`DBT_REGISTER_FILE_BASE .. +32`), where `x0` is physically grounded at `Word21::ZERO`.
//! 3. **Basic block compiler with REBEL-6 / Setun fusion**:
//!    - 32-bit constant fusion (`LUI + ADDI -> LI.T` and `AUIPC + ADDI -> AIPC.T`) into a single
//!      21-trit operation (since $[-2^{31}, 2^{31}-1] \subset [-5.23 \times 10^9, +5.23 \times 10^9]$).
//!    - Comparison and branch fusion (`SLT/SLTI/SLTU/SLTIU + BEQ/BNE x0 -> BCEG`) using the
//!      21-gate trichotomous comparator [`Word21::gate_cmp`].
//! 4. **Strict gate-level execution (Zero False Positives)**:
//!    - All additions, subtractions, effective address calculations, and 32-bit range wraps
//!      operate through the 21-stage Ripple-Carry adder ([`Word21::gate_ripple_carry_add`])
//!      and the physical inverter ([`Word21::gate_invert`]).
//!    - All signed and unsigned comparisons operate through the ternary comparator
//!      ([`Word21::gate_cmp`]).
//!    - Bitwise logic operations from binary legacy (`AND`, `OR`, `XOR`, `SLL`, `SRL`, `SRA`)
//!      pass through the hardware base converter by J. Ramil Alvarez (1974) in [`Cpu`],
//!      recording all physical carry propagations.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::cpu::{Cpu, Instruction, Opcode, PowerTelemetry};
use crate::elf::{
    read_elf_u16_le, read_elf_u32_le, read_elf_u8, write_elf_u8, Elf32Image, ElfError,
    LoadedElfLayout, DEFAULT_ELF_MEMORY_BASE, DEFAULT_RV32I_STACK_TOP,
};
use crate::memory::{Memory, MemoryError};
use crate::trit::Trit;
use crate::word::Word21;

/// Base address in Setun memory for the `RV32I` register file `x0..x31`.
pub const DBT_REGISTER_FILE_BASE: usize = 500;

/// Base address in Setun memory for the translated `Word21` instruction cache.
pub const DBT_CODE_CACHE_BASE: usize = 1_000;

/// Maximum capacity of the translated code cache in Setun memory.
pub const DBT_CODE_CACHE_LIMIT: usize = 90_000;

/// Upper limit of `i32` (`+2,147,483,647`) represented in [`Word21`].
const MAX_I32_WORD21: Word21 = Word21::new(i32::MAX as i64);

/// Lower limit of `i32` (`-2,147,483,648`) represented in [`Word21`].
const MIN_I32_WORD21: Word21 = Word21::new(i32::MIN as i64);

/// Modulus $2^{32} = +4,294,967,296$ represented in [`Word21`] (comfortably fits in $\pm 5.23 \times 10^9$).
const POS_MOD32_WORD21: Word21 = Word21::new(4_294_967_296);

/// Negative modulus $-2^{32} = -4,294,967,296$ represented in [`Word21`].
const NEG_MOD32_WORD21: Word21 = Word21::new(-4_294_967_296);

/// Standard ABI names of the 32 `RV32I` integer registers.
pub const RV32I_REG_ABI_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

/// Individual instruction from the `RV32I` ISA decoded from a 32-bit word.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Rv32iInstr {
    Lui { rd: u8, imm: i32 },
    Auipc { rd: u8, imm: i32 },
    Jal { rd: u8, offset: i32 },
    Jalr { rd: u8, rs1: u8, imm: i32 },
    Beq { rs1: u8, rs2: u8, offset: i32 },
    Bne { rs1: u8, rs2: u8, offset: i32 },
    Blt { rs1: u8, rs2: u8, offset: i32 },
    Bge { rs1: u8, rs2: u8, offset: i32 },
    Bltu { rs1: u8, rs2: u8, offset: i32 },
    Bgeu { rs1: u8, rs2: u8, offset: i32 },
    Lb { rd: u8, rs1: u8, imm: i32 },
    Lh { rd: u8, rs1: u8, imm: i32 },
    Lw { rd: u8, rs1: u8, imm: i32 },
    Lbu { rd: u8, rs1: u8, imm: i32 },
    Lhu { rd: u8, rs1: u8, imm: i32 },
    Sb { rs1: u8, rs2: u8, imm: i32 },
    Sh { rs1: u8, rs2: u8, imm: i32 },
    Sw { rs1: u8, rs2: u8, imm: i32 },
    Addi { rd: u8, rs1: u8, imm: i32 },
    Slti { rd: u8, rs1: u8, imm: i32 },
    Sltiu { rd: u8, rs1: u8, imm: i32 },
    Xori { rd: u8, rs1: u8, imm: i32 },
    Ori { rd: u8, rs1: u8, imm: i32 },
    Andi { rd: u8, rs1: u8, imm: i32 },
    Slli { rd: u8, rs1: u8, shamt: u8 },
    Srli { rd: u8, rs1: u8, shamt: u8 },
    Srai { rd: u8, rs1: u8, shamt: u8 },
    Add { rd: u8, rs1: u8, rs2: u8 },
    Sub { rd: u8, rs1: u8, rs2: u8 },
    Sll { rd: u8, rs1: u8, rs2: u8 },
    Slt { rd: u8, rs1: u8, rs2: u8 },
    Sltu { rd: u8, rs1: u8, rs2: u8 },
    Xor { rd: u8, rs1: u8, rs2: u8 },
    Srl { rd: u8, rs1: u8, rs2: u8 },
    Sra { rd: u8, rs1: u8, rs2: u8 },
    Or { rd: u8, rs1: u8, rs2: u8 },
    And { rd: u8, rs1: u8, rs2: u8 },
    // RV32M Extension (Integer Multiplication and Division)
    Mul { rd: u8, rs1: u8, rs2: u8 },
    Mulh { rd: u8, rs1: u8, rs2: u8 },
    Mulhsu { rd: u8, rs1: u8, rs2: u8 },
    Mulhu { rd: u8, rs1: u8, rs2: u8 },
    Div { rd: u8, rs1: u8, rs2: u8 },
    Divu { rd: u8, rs1: u8, rs2: u8 },
    Rem { rd: u8, rs1: u8, rs2: u8 },
    Remu { rd: u8, rs1: u8, rs2: u8 },
    Fence,
    Ecall,
    Ebreak,
}

impl Rv32iInstr {
    /// Decodes a 32-bit Little-Endian binary word into an [`Rv32iInstr`].
    pub fn decode(raw: u32) -> Result<Self, DbtError> {
        let opcode = raw & 0x7F;
        let rd = ((raw >> 7) & 0x1F) as u8;
        let funct3 = (raw >> 12) & 0x07;
        let rs1 = ((raw >> 15) & 0x1F) as u8;
        let rs2 = ((raw >> 20) & 0x1F) as u8;
        let funct7 = (raw >> 25) & 0x7F;

        match opcode {
            0x37 => {
                let imm = (raw & 0xFFFF_F000) as i32;
                Ok(Self::Lui { rd, imm })
            }
            0x17 => {
                let imm = (raw & 0xFFFF_F000) as i32;
                Ok(Self::Auipc { rd, imm })
            }
            0x6F => {
                let offset = decode_j_imm(raw);
                Ok(Self::Jal { rd, offset })
            }
            0x67 => {
                if funct3 != 0 {
                    return Err(DbtError::IllegalInstruction { raw, pc: 0 });
                }
                let imm = (raw as i32) >> 20;
                Ok(Self::Jalr { rd, rs1, imm })
            }
            0x63 => {
                let offset = decode_b_imm(raw);
                match funct3 {
                    0 => Ok(Self::Beq { rs1, rs2, offset }),
                    1 => Ok(Self::Bne { rs1, rs2, offset }),
                    4 => Ok(Self::Blt { rs1, rs2, offset }),
                    5 => Ok(Self::Bge { rs1, rs2, offset }),
                    6 => Ok(Self::Bltu { rs1, rs2, offset }),
                    7 => Ok(Self::Bgeu { rs1, rs2, offset }),
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x03 => {
                let imm = (raw as i32) >> 20;
                match funct3 {
                    0 => Ok(Self::Lb { rd, rs1, imm }),
                    1 => Ok(Self::Lh { rd, rs1, imm }),
                    2 => Ok(Self::Lw { rd, rs1, imm }),
                    4 => Ok(Self::Lbu { rd, rs1, imm }),
                    5 => Ok(Self::Lhu { rd, rs1, imm }),
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x23 => {
                let imm = decode_s_imm(raw);
                match funct3 {
                    0 => Ok(Self::Sb { rs1, rs2, imm }),
                    1 => Ok(Self::Sh { rs1, rs2, imm }),
                    2 => Ok(Self::Sw { rs1, rs2, imm }),
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x13 => {
                let imm = (raw as i32) >> 20;
                let shamt = ((raw >> 20) & 0x1F) as u8;
                match funct3 {
                    0 => Ok(Self::Addi { rd, rs1, imm }),
                    2 => Ok(Self::Slti { rd, rs1, imm }),
                    3 => Ok(Self::Sltiu { rd, rs1, imm }),
                    4 => Ok(Self::Xori { rd, rs1, imm }),
                    6 => Ok(Self::Ori { rd, rs1, imm }),
                    7 => Ok(Self::Andi { rd, rs1, imm }),
                    1 => {
                        if funct7 != 0 {
                            return Err(DbtError::IllegalInstruction { raw, pc: 0 });
                        }
                        Ok(Self::Slli { rd, rs1, shamt })
                    }
                    5 => match funct7 {
                        0x00 => Ok(Self::Srli { rd, rs1, shamt }),
                        0x20 => Ok(Self::Srai { rd, rs1, shamt }),
                        _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                    },
                    _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
                }
            }
            0x33 => match (funct3, funct7) {
                (0, 0x00) => Ok(Self::Add { rd, rs1, rs2 }),
                (0, 0x20) => Ok(Self::Sub { rd, rs1, rs2 }),
                (1, 0x00) => Ok(Self::Sll { rd, rs1, rs2 }),
                (2, 0x00) => Ok(Self::Slt { rd, rs1, rs2 }),
                (3, 0x00) => Ok(Self::Sltu { rd, rs1, rs2 }),
                (4, 0x00) => Ok(Self::Xor { rd, rs1, rs2 }),
                (5, 0x00) => Ok(Self::Srl { rd, rs1, rs2 }),
                (5, 0x20) => Ok(Self::Sra { rd, rs1, rs2 }),
                (6, 0x00) => Ok(Self::Or { rd, rs1, rs2 }),
                (7, 0x00) => Ok(Self::And { rd, rs1, rs2 }),
                // RV32M Extension (funct7 == 0x01)
                (0, 0x01) => Ok(Self::Mul { rd, rs1, rs2 }),
                (1, 0x01) => Ok(Self::Mulh { rd, rs1, rs2 }),
                (2, 0x01) => Ok(Self::Mulhsu { rd, rs1, rs2 }),
                (3, 0x01) => Ok(Self::Mulhu { rd, rs1, rs2 }),
                (4, 0x01) => Ok(Self::Div { rd, rs1, rs2 }),
                (5, 0x01) => Ok(Self::Divu { rd, rs1, rs2 }),
                (6, 0x01) => Ok(Self::Rem { rd, rs1, rs2 }),
                (7, 0x01) => Ok(Self::Remu { rd, rs1, rs2 }),
                _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
            },
            0x0F => Ok(Self::Fence),
            0x73 => {
                if funct3 == 0 {
                    match raw >> 20 {
                        0 => Ok(Self::Ecall),
                        1 => Ok(Self::Ebreak),
                        _ => Ok(Self::Fence),
                    }
                } else {
                    // CSR instructions (when occurring in bare-metal prologues) treated as NOP
                    Ok(Self::Fence)
                }
            }
            _ => Err(DbtError::IllegalInstruction { raw, pc: 0 }),
        }
    }
}

/// Comparison mode in fused trichotomous instruction `BCEG` (`SLT* + BEQ/BNE`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FusedCmpKind {
    /// Compare two signed registers (`SLT`).
    RegSigned { rs1: u8, rs2: u8 },
    /// Compare two unsigned registers (`SLTU`).
    RegUnsigned { rs1: u8, rs2: u8 },
    /// Compare signed register and immediate (`SLTI`).
    ImmSigned { rs1: u8, imm: i32 },
    /// Compare unsigned register and immediate (`SLTIU`).
    ImmUnsigned { rs1: u8, imm: i32 },
}

/// Operation translated to the `Setun-Doom` ternary machine (`REBEL-6 / Setun ISA`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SetunTranslatedOp {
    /// Direct load of 32-bit constant in 21 trits (`LI.T`), fusing `LUI + ADDI`.
    LiT {
        rd: u8,
        imm32: i32,
        intermediate_hi20: Option<i32>,
        rv32i_instrs: u8,
    },
    /// Load PC-relative address in 21 trits (`AIPC.T`), fusing `AUIPC + ADDI`.
    AipcT {
        rd: u8,
        target_addr: i32,
        intermediate_hi20: Option<i32>,
        rv32i_instrs: u8,
    },
    /// Fused trichotomous conditional branch (`SLT/SLTI/SLTU/SLTIU [+ LI 0|1] + BEQ/BNE -> BCEG`).
    BcegFused {
        cmp_rd: u8,
        aux_li: Option<(u8, i32)>,
        kind: FusedCmpKind,
        branch_if_less: bool,
        target_pc: u32,
        fallthrough_pc: u32,
        rv32i_instrs: u8,
    },
    /// Pure sign inversion via Setun ternary gates (`NEG rd, rs2` / `STI.T`).
    NegT { rd: u8, rs: u8 },
    /// Base instruction mapped 1-to-1 to the 21-trit ternary ALU.
    Single { instr: Rv32iInstr, pc: u32 },
}

/// Basic block translated and stored in dynamic translation cache.
#[derive(Clone, Debug)]
pub struct TranslatedBasicBlock {
    /// Starting block address in `RV32I` virtual address space.
    pub start_pc: u32,
    /// Ternary Setun operations generated for this block.
    pub ops: Vec<SetunTranslatedOp>,
    /// Base address in Setun memory (`DBT_CODE_CACHE_BASE`) where `Instruction`s were written.
    pub setun_cache_addr: usize,
}

/// Execution or translation errors from the DBT engine.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbtError {
    /// Error loading ELF32 image.
    Elf(ElfError),
    /// Access error in Setun linear memory.
    Memory(MemoryError),
    /// Unknown or corrupted binary instruction.
    IllegalInstruction { raw: u32, pc: u32 },
    /// Stopped at `EBREAK`.
    Breakpoint { pc: u32 },
    /// Maximum cycle/instruction limit exceeded (prevents infinite loop in tests).
    StepLimitExceeded(u64),
}

impl fmt::Display for DbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Elf(err) => write!(f, "DBT ELF loader error: {err}"),
            Self::Memory(err) => write!(f, "DBT memory access error: {err}"),
            Self::IllegalInstruction { raw, pc } => {
                write!(f, "Illegal instruction 0x{raw:08X} at PC 0x{pc:08X}")
            }
            Self::Breakpoint { pc } => write!(f, "Breakpoint reached at PC 0x{pc:08X}"),
            Self::StepLimitExceeded(limit) => write!(f, "Step limit exceeded: {limit} cycles"),
        }
    }
}

impl Error for DbtError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Elf(err) => Some(err),
            Self::Memory(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ElfError> for DbtError {
    fn from(e: ElfError) -> Self {
        Self::Elf(e)
    }
}

impl From<MemoryError> for DbtError {
    fn from(e: MemoryError) -> Self {
        Self::Memory(e)
    }
}

/// Exit reason after executing a translated `RV32I` binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbtExitReason {
    /// Terminated via `ecall` (`riscv-tests` convention or `exit` syscall).
    Ecall {
        /// Exit code in `a0` (`x10`, where `0` = pass in `riscv-tests`).
        a0_exit_code: i32,
        /// Current test ID in `gp` (`x3`).
        gp_test_id: i32,
        /// Syscall number in `a7` (`x17`).
        a7_syscall: i32,
    },
    /// Returned from `main` to sentinel `ra` address.
    ReturnedFromMain { a0_return_value: i32 },
}

/// Comprehensive execution and telemetry report for an `RV32I` program on `Setun-Doom`.
#[derive(Clone, Debug)]
pub struct DbtExecutionReport {
    /// Stop reason and return codes.
    pub exit: DbtExitReason,
    /// Total equivalent original binary `RV32I` instructions executed.
    pub rv32i_instructions_executed: u64,
    /// Total ternary `Setun-Doom` instructions actually executed (after `LI.T`, `AIPC.T`, `BCEG` fusions).
    pub setun_instructions_executed: u64,
    /// Quantity of immediate fusions (`LUI/AUIPC + ADDI`) executed.
    pub immediate_fusions_executed: u64,
    /// Quantity of trichotomous branch fusions (`SLT* + BEQ/BNE -> BCEG`) executed.
    pub branch_fusions_executed: u64,
    /// Quantity of direct Setun sign inversions (`NEG rd, rs2` / `STI.T`) executed.
    pub neg_fusions_executed: u64,
    /// Physical state transition telemetry (*Trit Flips*, *Half/Full-Swings*, *Bit Flips*, and *Gate Carries*).
    pub telemetry: PowerTelemetry,
    /// Captured output from `stdout` (`write` syscall).
    pub stdout_buffer: Vec<u8>,
}

impl DbtExecutionReport {
    /// Percentage reduction of instructions executed (`Setun` vs `RV32I`).
    pub fn instruction_reduction_pct(&self) -> f64 {
        if self.rv32i_instructions_executed == 0 {
            return 0.0;
        }
        let r = self.rv32i_instructions_executed as f64;
        let s = self.setun_instructions_executed as f64;
        ((s - r) / r) * 100.0
    }

    /// Checks if the official RISC-V test suite passed (`a0 == 0` and `gp > 0`).
    pub fn is_riscv_test_pass(&self) -> bool {
        match self.exit {
            DbtExitReason::Ecall {
                a0_exit_code,
                gp_test_id,
                ..
            } => a0_exit_code == 0 && gp_test_id > 0,
            DbtExitReason::ReturnedFromMain { a0_return_value } => a0_return_value == 0,
        }
    }
}

/// Sentinel value placed in `ra` (`x1`) at `main` start to detect clean return
/// (same technique as the `REBEL-6` simulator, where only a single trit is set: $-3^{16} = -43,046,721$).
pub const SENTINEL_RETURN_PC: u32 = 0xFFFFFFFE;

/// Dynamic Binary Translation (`DBT`) engine executing `ELF32` `RV32I` binaries on Setun CPU and Memory.
pub struct SetunDbtEngine {
    /// Balanced ternary Setun-Doom CPU (accumulator, R register, Alvarez converter, and telemetry).
    pub cpu: Cpu,
    /// Linear ternary memory of $3^{14} = 4,782,969$ words.
    pub memory: Memory,
    /// Architectural register file of 32 `x0..x31` registers represented in [`Word21`].
    pub regs: [Word21; 32],
    /// Program counter (`PC`) in `RV32I` virtual address space.
    pub pc: u32,
    /// Loaded ELF32 image layout in Setun memory.
    pub layout: LoadedElfLayout,
    /// Set of addresses that are static targets of jumps/branches (for fusion safety).
    branch_targets: HashSet<u32>,
    /// Cache of already translated basic blocks (`start_pc -> TranslatedBasicBlock`).
    block_cache: HashMap<u32, TranslatedBasicBlock>,
    /// Next free address in the `DBT_CODE_CACHE_BASE` region of Setun memory.
    next_code_cache_addr: usize,
    /// Instruction and fusion counters.
    pub rv32i_instructions_executed: u64,
    pub setun_instructions_executed: u64,
    pub immediate_fusions_executed: u64,
    pub branch_fusions_executed: u64,
    pub neg_fusions_executed: u64,
    /// Console buffer for `write` syscall.
    pub stdout_buffer: Vec<u8>,
}

impl SetunDbtEngine {
    /// Creates a new `SetunDbtEngine` instance and loads an `ELF32` binary into ternary memory.
    pub fn from_elf_bytes(elf_bytes: &[u8]) -> Result<Self, DbtError> {
        let image = Elf32Image::parse(elf_bytes)?;
        let mut memory = Memory::new();
        let layout = image.load_into_setun_memory(&mut memory, DEFAULT_ELF_MEMORY_BASE)?;

        let branch_targets = scan_static_branch_targets(&memory, &layout, &image);

        let mut engine = Self {
            cpu: Cpu::new(),
            memory,
            regs: [Word21::ZERO; 32],
            pc: layout.entry_point,
            layout,
            branch_targets,
            block_cache: HashMap::new(),
            next_code_cache_addr: DBT_CODE_CACHE_BASE,
            rv32i_instructions_executed: 0,
            setun_instructions_executed: 0,
            immediate_fusions_executed: 0,
            branch_fusions_executed: 0,
            neg_fusions_executed: 0,
            stdout_buffer: Vec::new(),
        };

        // Initialize Stack Pointer (sp = x2) and sentinel Return Address (ra = x1)
        // synchronized in Setun memory without polluting user program telemetry.
        let sp_word = Word21::new(DEFAULT_RV32I_STACK_TOP as i64);
        let ra_word = Word21::new(SENTINEL_RETURN_PC as i32 as i64);
        engine.regs[2] = sp_word;
        engine.regs[1] = ra_word;
        for r in 0..32 {
            engine
                .memory
                .write_word((DBT_REGISTER_FILE_BASE + r) as i64, engine.regs[r])?;
        }

        Ok(engine)
    }

    /// Reads register `x0..x31` as [`Word21`] (`x0` always returns `Word21::ZERO`).
    #[inline(always)]
    pub fn read_reg(&self, reg: u8) -> Word21 {
        let idx = (reg & 0x1F) as usize;
        if idx == 0 {
            Word21::ZERO
        } else {
            self.regs[idx]
        }
    }

    /// Writes [`Word21`] into register `rd` (`x1..x31`), updates corresponding cell
    /// in Setun memory (`DBT_REGISTER_FILE_BASE + rd`), and records *Trit Flips* vs *Bit Flips*.
    #[inline]
    pub fn write_reg(&mut self, rd: u8, new_val: Word21) -> Result<(), MemoryError> {
        let idx = (rd & 0x1F) as usize;
        if idx == 0 {
            return Ok(());
        }
        let old_val = self.regs[idx];
        self.regs[idx] = new_val;
        self.memory
            .write_word((DBT_REGISTER_FILE_BASE + idx) as i64, new_val)?;
        self.cpu.telemetry.record_transition(old_val, new_val);
        Ok(())
    }

    /// Converts a binary `i32` integer to [`Word21`] through the hardware converter
    /// by J. Ramil Alvarez (1974) (`Word21::alvarez_from_i32_gates`), accumulating all
    /// carry propagations from `Trit::full_add` gates.
    #[inline]
    pub fn gate_alvarez_bin_to_ter(&mut self, val: i32) -> Word21 {
        let (trits, carries) = Word21::alvarez_from_i32_gates(val);
        self.cpu.telemetry.adder_carry_propagations += carries as u64;
        Word21::from_trits(&trits)
    }

    /// Extracts the 32-bit two's complement value from a [`Word21`].
    #[inline(always)]
    pub fn gate_alvarez_ter_to_bin(&self, w: Word21) -> i32 {
        w.to_i32_saturating()
    }

    /// Adds two `Word21` words using the physical 21-gate Ripple-Carry adder (`Trit::full_add`)
    /// and applies 32-bit wrapping (`[-2^31, 2^31 - 1]`) through ternary gates.
    #[inline]
    pub fn gate_add_i32(&mut self, a: Word21, b: Word21) -> Word21 {
        let (sum, _cout, carries) = a.gate_ripple_carry_add(b, Trit::Zero);
        self.cpu.telemetry.adder_carry_propagations += carries as u64;
        self.cpu.cycles += 1;

        if sum.gate_cmp_t(MAX_I32_WORD21) == Trit::Positive {
            let (wrapped, _c2, carries2) = sum.gate_ripple_carry_add(NEG_MOD32_WORD21, Trit::Zero);
            self.cpu.telemetry.adder_carry_propagations += carries2 as u64;
            wrapped
        } else if sum.gate_cmp_t(MIN_I32_WORD21) == Trit::Negative {
            let (wrapped, _c2, carries2) = sum.gate_ripple_carry_add(POS_MOD32_WORD21, Trit::Zero);
            self.cpu.telemetry.adder_carry_propagations += carries2 as u64;
            wrapped
        } else {
            sum
        }
    }

    /// Subtracts `b` from `a` by inverting the 21 wires of `b` (`Word21::gate_invert_trits`) and adding through
    /// the 21-stage Ripple-Carry adder (`gate_add_i32`).
    #[inline]
    pub fn gate_sub_i32(&mut self, a: Word21, b: Word21) -> Word21 {
        let neg_b = b.gate_invert_trits();
        self.gate_add_i32(a, neg_b)
    }

    /// Converts a signed `Word21` (`[-2^31, 2^31 - 1]`) to its 32-bit unsigned magnitude
    /// (`[0, 2^32 - 1]`) inside a `Word21` using the ternary Ripple-Carry adder.
    #[inline]
    pub fn gate_to_unsigned32_word21(&mut self, w: Word21) -> Word21 {
        if w.sign_trit() == Trit::Negative {
            let (pos, _c, carries) = w.gate_ripple_carry_add(POS_MOD32_WORD21, Trit::Zero);
            self.cpu.telemetry.adder_carry_propagations += carries as u64;
            pos
        } else {
            w
        }
    }

    /// Compares two `Word21` values as unsigned 32-bit integers using the 21-gate comparator `gate_cmp_t`.
    #[inline]
    pub fn gate_cmp_u32(&mut self, a: Word21, b: Word21) -> Trit {
        let ua = self.gate_to_unsigned32_word21(a);
        let ub = self.gate_to_unsigned32_word21(b);
        ua.gate_cmp_t(ub)
    }

    /// Translates (or retrieves from cache) the basic block starting at `start_pc`.
    pub fn get_or_translate_block(
        &mut self,
        start_pc: u32,
    ) -> Result<TranslatedBasicBlock, DbtError> {
        if let Some(block) = self.block_cache.get(&start_pc) {
            return Ok(block.clone());
        }

        let mut ops = Vec::with_capacity(16);
        let mut cur_pc = start_pc;
        let cache_addr = self.next_code_cache_addr;

        for _ in 0..64 {
            let raw0 = read_elf_u32_le(&self.memory, self.layout.setun_base_offset, cur_pc)?;
            let instr0 = Rv32iInstr::decode(raw0).map_err(|_| DbtError::IllegalInstruction {
                raw: raw0,
                pc: cur_pc,
            })?;

            let next_pc = cur_pc.wrapping_add(4);
            let can_fuse_next = !self.branch_targets.contains(&next_pc);

            // 1. Attempt LUI + ADDI -> LI.T fusion (complete 32-bit constant in 21 trits)
            if let Rv32iInstr::Lui { rd, imm: hi20 } = instr0 {
                if rd != 0 && can_fuse_next {
                    if let Ok(raw1) =
                        read_elf_u32_le(&self.memory, self.layout.setun_base_offset, next_pc)
                    {
                        if let Ok(Rv32iInstr::Addi {
                            rd: rd2,
                            rs1,
                            imm: lo12,
                        }) = Rv32iInstr::decode(raw1)
                        {
                            if rd2 == rd && rs1 == rd {
                                let full_imm = hi20.wrapping_add(lo12);
                                ops.push(SetunTranslatedOp::LiT {
                                    rd,
                                    imm32: full_imm,
                                    intermediate_hi20: Some(hi20),
                                    rv32i_instrs: 2,
                                });
                                self.emit_setun_code_word(Opcode::Load, rd as i64);
                                cur_pc = cur_pc.wrapping_add(8);
                                if self.branch_targets.contains(&cur_pc) {
                                    break;
                                }
                                continue;
                            }
                        }
                    }
                }
            }

            // 2. Attempt AUIPC + ADDI -> AIPC.T fusion (complete PC-relative address in 21 trits)
            if let Rv32iInstr::Auipc { rd, imm: hi20 } = instr0 {
                if rd != 0 && can_fuse_next {
                    if let Ok(raw1) =
                        read_elf_u32_le(&self.memory, self.layout.setun_base_offset, next_pc)
                    {
                        if let Ok(Rv32iInstr::Addi {
                            rd: rd2,
                            rs1,
                            imm: lo12,
                        }) = Rv32iInstr::decode(raw1)
                        {
                            if rd2 == rd && rs1 == rd {
                                let hi_addr = (cur_pc as i32).wrapping_add(hi20);
                                let full_addr = hi_addr.wrapping_add(lo12);
                                ops.push(SetunTranslatedOp::AipcT {
                                    rd,
                                    target_addr: full_addr,
                                    intermediate_hi20: Some(hi_addr),
                                    rv32i_instrs: 2,
                                });
                                self.emit_setun_code_word(Opcode::Load, rd as i64);
                                cur_pc = cur_pc.wrapping_add(8);
                                if self.branch_targets.contains(&cur_pc) {
                                    break;
                                }
                                continue;
                            }
                        }
                    }
                }
            }

            // 3. Attempt Trichotomous SLT/SLTI/SLTU/SLTIU + [LI 0|1] + BEQ/BNE -> BCEG fusion
            let slt_candidate = match instr0 {
                Rv32iInstr::Slt { rd, rs1, rs2 } if rd != 0 => {
                    Some((rd, FusedCmpKind::RegSigned { rs1, rs2 }))
                }
                Rv32iInstr::Sltu { rd, rs1, rs2 } if rd != 0 => {
                    Some((rd, FusedCmpKind::RegUnsigned { rs1, rs2 }))
                }
                Rv32iInstr::Slti { rd, rs1, imm } if rd != 0 => {
                    Some((rd, FusedCmpKind::ImmSigned { rs1, imm }))
                }
                Rv32iInstr::Sltiu { rd, rs1, imm } if rd != 0 => {
                    Some((rd, FusedCmpKind::ImmUnsigned { rs1, imm }))
                }
                _ => None,
            };

            if let Some((cmp_rd, kind)) = slt_candidate {
                if can_fuse_next {
                    if let Ok(raw1) =
                        read_elf_u32_le(&self.memory, self.layout.setun_base_offset, next_pc)
                    {
                        if let Ok(instr1) = Rv32iInstr::decode(raw1) {
                            // Case 3a: 2-instruction window (SLT* + BEQ/BNE cmp_rd, x0, offset)
                            let branch_match_2 = match instr1 {
                                Rv32iInstr::Bne { rs1, rs2, offset }
                                    if (rs1 == cmp_rd && rs2 == 0)
                                        || (rs1 == 0 && rs2 == cmp_rd) =>
                                {
                                    Some((true, offset))
                                }
                                Rv32iInstr::Beq { rs1, rs2, offset }
                                    if (rs1 == cmp_rd && rs2 == 0)
                                        || (rs1 == 0 && rs2 == cmp_rd) =>
                                {
                                    Some((false, offset))
                                }
                                _ => None,
                            };

                            if let Some((branch_if_less, offset)) = branch_match_2 {
                                let target_pc = (next_pc as i32).wrapping_add(offset) as u32;
                                let fallthrough_pc = cur_pc.wrapping_add(8);
                                ops.push(SetunTranslatedOp::BcegFused {
                                    cmp_rd,
                                    kind,
                                    aux_li: None,
                                    branch_if_less,
                                    target_pc,
                                    fallthrough_pc,
                                    rv32i_instrs: 2,
                                });
                                self.emit_setun_code_word(Opcode::Bceg, offset as i64);
                                break;
                            }

                            // Case 3b: 3-instruction window (SLT* + LI r_exp, 0|1 + BEQ/BNE cmp_rd, r_exp, offset)
                            if let Rv32iInstr::Addi {
                                rd: exp_rd,
                                rs1: 0,
                                imm: exp_imm @ (0 | 1),
                            } = instr1
                            {
                                let third_pc = cur_pc.wrapping_add(8);
                                if exp_rd != 0
                                    && exp_rd != cmp_rd
                                    && !self.branch_targets.contains(&third_pc)
                                {
                                    if let Ok(raw2) = read_elf_u32_le(
                                        &self.memory,
                                        self.layout.setun_base_offset,
                                        third_pc,
                                    ) {
                                        if let Ok(instr2) = Rv32iInstr::decode(raw2) {
                                            let branch_match_3 = match instr2 {
                                                Rv32iInstr::Bne { rs1, rs2, offset }
                                                    if (rs1 == cmp_rd && rs2 == exp_rd)
                                                        || (rs1 == exp_rd && rs2 == cmp_rd) =>
                                                {
                                                    // BNE branches when cmp_rd != exp_imm:
                                                    // if exp_imm == 0, branch when cmp_rd == 1 (is_less == true)
                                                    // if exp_imm == 1, branch when cmp_rd == 0 (is_less == false)
                                                    Some((exp_imm == 0, offset))
                                                }
                                                Rv32iInstr::Beq { rs1, rs2, offset }
                                                    if (rs1 == cmp_rd && rs2 == exp_rd)
                                                        || (rs1 == exp_rd && rs2 == cmp_rd) =>
                                                {
                                                    // BEQ branches when cmp_rd == exp_imm:
                                                    // if exp_imm == 1, branch when cmp_rd == 1 (is_less == true)
                                                    // if exp_imm == 0, branch when cmp_rd == 0 (is_less == false)
                                                    Some((exp_imm == 1, offset))
                                                }
                                                _ => None,
                                            };

                                            if let Some((branch_if_less, offset)) = branch_match_3 {
                                                let target_pc =
                                                    (third_pc as i32).wrapping_add(offset) as u32;
                                                let fallthrough_pc = cur_pc.wrapping_add(12);
                                                ops.push(SetunTranslatedOp::BcegFused {
                                                    cmp_rd,
                                                    kind,
                                                    aux_li: Some((exp_rd, exp_imm)),
                                                    branch_if_less,
                                                    target_pc,
                                                    fallthrough_pc,
                                                    rv32i_instrs: 3,
                                                });
                                                self.emit_setun_code_word(
                                                    Opcode::Bceg,
                                                    offset as i64,
                                                );
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 4. Peephole Optimization: Pure Setun sign inversion (NEG rd, rs2 / STI.T)
            // In standard RV32I, 'neg rd, rs' is a pseudo-instruction translated as 'sub rd, x0, rs'.
            // On Setun, this dispenses with adders and carry propagation altogether,
            // executing directly via physical wire-by-wire inversion of 21 trits (+1 <-> -1).
            if let Rv32iInstr::Sub { rd, rs1: 0, rs2 } = instr0 {
                ops.push(SetunTranslatedOp::NegT { rd, rs: rs2 });
                self.emit_setun_code_word(Opcode::Neg, rd as i64);
                cur_pc = next_pc;
                if self.branch_targets.contains(&cur_pc) {
                    break;
                }
                continue;
            }

            // 5. Direct 1-to-1 translation to Setun ternary operation
            ops.push(SetunTranslatedOp::Single {
                instr: instr0,
                pc: cur_pc,
            });
            self.emit_setun_code_word(map_rv32i_to_setun_opcode(&instr0), cur_pc as i64);

            let is_terminator = matches!(
                instr0,
                Rv32iInstr::Jal { .. }
                    | Rv32iInstr::Jalr { .. }
                    | Rv32iInstr::Beq { .. }
                    | Rv32iInstr::Bne { .. }
                    | Rv32iInstr::Blt { .. }
                    | Rv32iInstr::Bge { .. }
                    | Rv32iInstr::Bltu { .. }
                    | Rv32iInstr::Bgeu { .. }
                    | Rv32iInstr::Ecall
                    | Rv32iInstr::Ebreak
            );

            cur_pc = next_pc;
            if is_terminator || self.branch_targets.contains(&cur_pc) {
                break;
            }
        }

        let block = TranslatedBasicBlock {
            start_pc,
            ops,
            setun_cache_addr: cache_addr,
        };
        self.block_cache.insert(start_pc, block.clone());
        Ok(block)
    }

    /// Emits a 21-trit encoded instruction into the `DBT_CODE_CACHE_BASE` region of Setun memory.
    fn emit_setun_code_word(&mut self, opcode: Opcode, operand: i64) {
        if self.next_code_cache_addr < DBT_CODE_CACHE_LIMIT {
            let clamped = operand.clamp(-7_000_000, 7_000_000);
            let instr = Instruction::new(opcode, 0, clamped);
            let _ = self
                .memory
                .write_word(self.next_code_cache_addr as i64, instr.encode());
            self.next_code_cache_addr += 1;
        }
    }

    /// Executes the translated `RV32I` program until encountering an exit call (`ecall` / `main` return)
    /// or reaching `max_steps`.
    pub fn run(&mut self, max_steps: u64) -> Result<DbtExecutionReport, DbtError> {
        while self.setun_instructions_executed < max_steps {
            if self.pc == SENTINEL_RETURN_PC {
                let a0_val = self.read_reg(10).to_i32_saturating();
                return Ok(self.build_report(DbtExitReason::ReturnedFromMain {
                    a0_return_value: a0_val,
                }));
            }

            let block = self.get_or_translate_block(self.pc)?;

            for op in &block.ops {
                match *op {
                    SetunTranslatedOp::LiT {
                        rd,
                        imm32,
                        intermediate_hi20,
                        rv32i_instrs,
                    } => {
                        // In unfused binary RV32I reference, LUI writes hi20 and ADDI writes imm32.
                        // On Setun (LI.T), we write the 21-trit Word21 directly in a single cycle.
                        if let Some(hi20) = intermediate_hi20 {
                            if rd != 0 {
                                let old_bin = self.read_reg(rd).to_i32_saturating();
                                let extra_flips =
                                    ((old_bin ^ hi20).count_ones() + (hi20 ^ imm32).count_ones())
                                        as u64;
                                let direct_flips = (old_bin ^ imm32).count_ones() as u64;
                                self.cpu.telemetry.binary32_bit_flips +=
                                    extra_flips.saturating_sub(direct_flips);
                            }
                        }
                        self.write_reg(rd, Word21::from_i32(imm32))?;
                        self.rv32i_instructions_executed += rv32i_instrs as u64;
                        self.setun_instructions_executed += 1;
                        self.immediate_fusions_executed += 1;
                        self.cpu.cycles += 1;
                        self.pc = self.pc.wrapping_add((rv32i_instrs as u32) * 4);
                    }

                    SetunTranslatedOp::AipcT {
                        rd,
                        target_addr,
                        intermediate_hi20,
                        rv32i_instrs,
                    } => {
                        if let Some(hi20) = intermediate_hi20 {
                            if rd != 0 {
                                let old_bin = self.read_reg(rd).to_i32_saturating();
                                let extra_flips = ((old_bin ^ hi20).count_ones()
                                    + (hi20 ^ target_addr).count_ones())
                                    as u64;
                                let direct_flips = (old_bin ^ target_addr).count_ones() as u64;
                                self.cpu.telemetry.binary32_bit_flips +=
                                    extra_flips.saturating_sub(direct_flips);
                            }
                        }
                        self.write_reg(rd, Word21::from_i32(target_addr))?;
                        self.rv32i_instructions_executed += rv32i_instrs as u64;
                        self.setun_instructions_executed += 1;
                        self.immediate_fusions_executed += 1;
                        self.cpu.cycles += 1;
                        self.pc = self.pc.wrapping_add((rv32i_instrs as u32) * 4);
                    }

                    SetunTranslatedOp::BcegFused {
                        cmp_rd,
                        kind,
                        aux_li,
                        branch_if_less,
                        target_pc,
                        fallthrough_pc,
                        rv32i_instrs,
                    } => {
                        let cmp_trit = match kind {
                            FusedCmpKind::RegSigned { rs1, rs2 } => {
                                let a = self.read_reg(rs1);
                                let b = self.read_reg(rs2);
                                a.gate_cmp_t(b)
                            }
                            FusedCmpKind::RegUnsigned { rs1, rs2 } => {
                                let a = self.read_reg(rs1);
                                let b = self.read_reg(rs2);
                                self.gate_cmp_u32(a, b)
                            }
                            FusedCmpKind::ImmSigned { rs1, imm } => {
                                let a = self.read_reg(rs1);
                                let b = Word21::from_i32(imm);
                                a.gate_cmp_t(b)
                            }
                            FusedCmpKind::ImmUnsigned { rs1, imm } => {
                                let a = self.read_reg(rs1);
                                let b = Word21::from_i32(imm);
                                self.gate_cmp_u32(a, b)
                            }
                        };

                        let is_less = cmp_trit == Trit::Negative;
                        let slt_val = if is_less {
                            Word21::new(1)
                        } else {
                            Word21::ZERO
                        };
                        self.write_reg(cmp_rd, slt_val)?;
                        if let Some((exp_rd, exp_imm)) = aux_li {
                            self.write_reg(exp_rd, Word21::from_i32(exp_imm))?;
                        }

                        self.rv32i_instructions_executed += rv32i_instrs as u64;
                        self.setun_instructions_executed += 1;
                        self.branch_fusions_executed += 1;
                        self.cpu.cycles += 1;

                        let take_branch = if branch_if_less { is_less } else { !is_less };
                        self.pc = if take_branch {
                            target_pc
                        } else {
                            fallthrough_pc
                        };
                        break;
                    }

                    SetunTranslatedOp::NegT { rd, rs } => {
                        let val = self.read_reg(rs);
                        let inverted = if val == MIN_I32_WORD21 {
                            // In 32-bit two's complement, -(-2^31) overflows back to -2^31
                            MIN_I32_WORD21
                        } else {
                            // For all other numbers in [-2^31 + 1, 2^31 - 1],
                            // Setun inversion is 100% symmetric through carryless trit gates.
                            val.gate_invert_trits()
                        };
                        self.write_reg(rd, inverted)?;
                        self.rv32i_instructions_executed += 1;
                        self.setun_instructions_executed += 1;
                        self.neg_fusions_executed += 1;
                        self.cpu.cycles += 1;
                        self.pc = self.pc.wrapping_add(4);
                    }

                    SetunTranslatedOp::Single { instr, pc } => {
                        self.pc = pc;
                        if let Some(exit_reason) = self.execute_single_instr(instr, pc)? {
                            return Ok(self.build_report(exit_reason));
                        }
                        if self.pc == SENTINEL_RETURN_PC {
                            let a0_val = self.read_reg(10).to_i32_saturating();
                            return Ok(self.build_report(DbtExitReason::ReturnedFromMain {
                                a0_return_value: a0_val,
                            }));
                        }
                        if self.pc != pc.wrapping_add(4) {
                            // Branch taken inside block
                            break;
                        }
                    }
                }
            }
        }

        Err(DbtError::StepLimitExceeded(max_steps))
    }

    /// Executes a single translated instruction using Setun ternary gate circuits.
    fn execute_single_instr(
        &mut self,
        instr: Rv32iInstr,
        pc: u32,
    ) -> Result<Option<DbtExitReason>, DbtError> {
        self.rv32i_instructions_executed += 1;
        self.setun_instructions_executed += 1;
        self.cpu.cycles += 1;

        let mut next_pc = pc.wrapping_add(4);

        match instr {
            Rv32iInstr::Lui { rd, imm } => {
                self.write_reg(rd, Word21::from_i32(imm))?;
            }
            Rv32iInstr::Auipc { rd, imm } => {
                let res = self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(imm));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Jal { rd, offset } => {
                let link = self.gate_add_i32(Word21::from_i32(pc as i32), Word21::new(4));
                let target =
                    self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                self.write_reg(rd, link)?;
                next_pc = target.to_i32_saturating() as u32;
            }
            Rv32iInstr::Jalr { rd, rs1, imm } => {
                // Compute target address from rs1 BEFORE overwriting rd (in case rd == rs1)
                let base = self.read_reg(rs1);
                let target_sum = self.gate_add_i32(base, Word21::from_i32(imm));
                let target_u32 = (target_sum.to_i32_saturating() as u32) & !1u32;
                let link = self.gate_add_i32(Word21::from_i32(pc as i32), Word21::new(4));
                self.write_reg(rd, link)?;
                next_pc = target_u32;
            }
            Rv32iInstr::Beq { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) == Trit::Zero {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bne { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) != Trit::Zero {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Blt { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) == Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bge { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if a.gate_cmp_t(b) != Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bltu { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if self.gate_cmp_u32(a, b) == Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Bgeu { rs1, rs2, offset } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                if self.gate_cmp_u32(a, b) != Trit::Negative {
                    let target =
                        self.gate_add_i32(Word21::from_i32(pc as i32), Word21::from_i32(offset));
                    next_pc = target.to_i32_saturating() as u32;
                }
            }
            Rv32iInstr::Lb { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let b = read_elf_u8(&self.memory, self.layout.setun_base_offset, vaddr)? as i8;
                let w = self.gate_alvarez_bin_to_ter(b as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lbu { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let b = read_elf_u8(&self.memory, self.layout.setun_base_offset, vaddr)?;
                let w = self.gate_alvarez_bin_to_ter(b as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lh { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let h = read_elf_u16_le(&self.memory, self.layout.setun_base_offset, vaddr)? as i16;
                let w = self.gate_alvarez_bin_to_ter(h as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lhu { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let h = read_elf_u16_le(&self.memory, self.layout.setun_base_offset, vaddr)?;
                let w = self.gate_alvarez_bin_to_ter(h as i32);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Lw { rd, rs1, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val =
                    read_elf_u32_le(&self.memory, self.layout.setun_base_offset, vaddr)? as i32;
                let w = self.gate_alvarez_bin_to_ter(val);
                self.write_reg(rd, w)?;
            }
            Rv32iInstr::Sb { rs1, rs2, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val_i32 = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                write_elf_u8(
                    &mut self.memory,
                    self.layout.setun_base_offset,
                    vaddr,
                    val_i32 as u8,
                )?;
            }
            Rv32iInstr::Sh { rs1, rs2, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val_i32 = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let bytes = (val_i32 as u16).to_le_bytes();
                write_elf_u8(
                    &mut self.memory,
                    self.layout.setun_base_offset,
                    vaddr,
                    bytes[0],
                )?;
                write_elf_u8(
                    &mut self.memory,
                    self.layout.setun_base_offset,
                    vaddr.wrapping_add(1),
                    bytes[1],
                )?;
            }
            Rv32iInstr::Sw { rs1, rs2, imm } => {
                let addr_w = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                let vaddr = addr_w.to_i32_saturating() as u32;
                let val_i32 = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let bytes = (val_i32 as u32).to_le_bytes();
                for i in 0..4 {
                    write_elf_u8(
                        &mut self.memory,
                        self.layout.setun_base_offset,
                        vaddr.wrapping_add(i),
                        bytes[i as usize],
                    )?;
                }
            }
            Rv32iInstr::Addi { rd, rs1, imm } => {
                let res = self.gate_add_i32(self.read_reg(rs1), Word21::from_i32(imm));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Slti { rd, rs1, imm } => {
                let a = self.read_reg(rs1);
                let b = Word21::from_i32(imm);
                let res = if a.gate_cmp_t(b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sltiu { rd, rs1, imm } => {
                let a = self.read_reg(rs1);
                let b = Word21::from_i32(imm);
                let res = if self.gate_cmp_u32(a, b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Xori { rd, rs1, imm } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin ^ imm);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Ori { rd, rs1, imm } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin | imm);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Andi { rd, rs1, imm } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin & imm);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Slli { rd, rs1, shamt } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res =
                    self.gate_alvarez_bin_to_ter(((a_bin as u32) << (shamt & 0x1F)) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Srli { rd, rs1, shamt } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res =
                    self.gate_alvarez_bin_to_ter(((a_bin as u32) >> (shamt & 0x1F)) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Srai { rd, rs1, shamt } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let res = self.gate_alvarez_bin_to_ter(a_bin >> (shamt & 0x1F));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Add { rd, rs1, rs2 } => {
                let res = self.gate_add_i32(self.read_reg(rs1), self.read_reg(rs2));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sub { rd, rs1, rs2 } => {
                let res = self.gate_sub_i32(self.read_reg(rs1), self.read_reg(rs2));
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sll { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32 & 0x1F;
                let res = self.gate_alvarez_bin_to_ter(((a_bin as u32) << b_bin) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Slt { rd, rs1, rs2 } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                let res = if a.gate_cmp_t(b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sltu { rd, rs1, rs2 } => {
                let a = self.read_reg(rs1);
                let b = self.read_reg(rs2);
                let res = if self.gate_cmp_u32(a, b) == Trit::Negative {
                    Word21::new(1)
                } else {
                    Word21::ZERO
                };
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Xor { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let res = self.gate_alvarez_bin_to_ter(a_bin ^ b_bin);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Srl { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32 & 0x1F;
                let res = self.gate_alvarez_bin_to_ter(((a_bin as u32) >> b_bin) as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Sra { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32 & 0x1F;
                let res = self.gate_alvarez_bin_to_ter(a_bin >> b_bin);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Or { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let res = self.gate_alvarez_bin_to_ter(a_bin | b_bin);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::And { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let res = self.gate_alvarez_bin_to_ter(a_bin & b_bin);
                self.write_reg(rd, res)?;
            }
            // RV32M Extension
            Rv32iInstr::Mul { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let prod = a_bin.wrapping_mul(b_bin);
                let res = self.gate_alvarez_bin_to_ter(prod);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Mulh { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1)) as i64;
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as i64;
                let prod = a_bin.wrapping_mul(b_bin);
                let hi = (prod >> 32) as i32;
                let res = self.gate_alvarez_bin_to_ter(hi);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Mulhsu { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1)) as i64;
                let b_bin = (self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32) as u64;
                let prod = a_bin.wrapping_mul(b_bin as i64);
                let hi = (prod >> 32) as i32;
                let res = self.gate_alvarez_bin_to_ter(hi);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Mulhu { rd, rs1, rs2 } => {
                let a_bin = (self.gate_alvarez_ter_to_bin(self.read_reg(rs1)) as u32) as u64;
                let b_bin = (self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32) as u64;
                let prod = a_bin.wrapping_mul(b_bin);
                let hi = (prod >> 32) as i32;
                let res = self.gate_alvarez_bin_to_ter(hi);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Div { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let q = if b_bin == 0 {
                    -1
                } else if a_bin == i32::MIN && b_bin == -1 {
                    i32::MIN
                } else {
                    a_bin.wrapping_div(b_bin)
                };
                let res = self.gate_alvarez_bin_to_ter(q);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Divu { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1)) as u32;
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32;
                let q = a_bin.checked_div(b_bin).unwrap_or(u32::MAX);
                let res = self.gate_alvarez_bin_to_ter(q as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Rem { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1));
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2));
                let r = if b_bin == 0 {
                    a_bin
                } else if a_bin == i32::MIN && b_bin == -1 {
                    0
                } else {
                    a_bin.wrapping_rem(b_bin)
                };
                let res = self.gate_alvarez_bin_to_ter(r);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Remu { rd, rs1, rs2 } => {
                let a_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs1)) as u32;
                let b_bin = self.gate_alvarez_ter_to_bin(self.read_reg(rs2)) as u32;
                let r = if b_bin == 0 {
                    a_bin
                } else {
                    a_bin % b_bin
                };
                let res = self.gate_alvarez_bin_to_ter(r as i32);
                self.write_reg(rd, res)?;
            }
            Rv32iInstr::Fence => {}
            Rv32iInstr::Ecall => {
                let a0 = self.read_reg(10).to_i32_saturating();
                let a1 = self.read_reg(11).to_i32_saturating() as u32;
                let a2 = self.read_reg(12).to_i32_saturating() as u32;
                let a7 = self.read_reg(17).to_i32_saturating();
                let gp = self.read_reg(3).to_i32_saturating();

                // Linux/Newlib write(fd=a0, buf=a1, count=a2) syscall when a7 == 64
                if a7 == 64 {
                    for i in 0..a2 {
                        if let Ok(b) = read_elf_u8(
                            &self.memory,
                            self.layout.setun_base_offset,
                            a1.wrapping_add(i),
                        ) {
                            self.stdout_buffer.push(b);
                        }
                    }
                    self.write_reg(10, Word21::from_i32(a2 as i32))?;
                } else {
                    // riscv-tests convention or exit(a0)
                    self.pc = next_pc;
                    return Ok(Some(DbtExitReason::Ecall {
                        a0_exit_code: a0,
                        gp_test_id: gp,
                        a7_syscall: a7,
                    }));
                }
            }
            Rv32iInstr::Ebreak => {
                return Err(DbtError::Breakpoint { pc });
            }
        }

        self.pc = next_pc;
        Ok(None)
    }

    fn build_report(&self, exit: DbtExitReason) -> DbtExecutionReport {
        DbtExecutionReport {
            exit,
            rv32i_instructions_executed: self.rv32i_instructions_executed,
            setun_instructions_executed: self.setun_instructions_executed,
            immediate_fusions_executed: self.immediate_fusions_executed,
            branch_fusions_executed: self.branch_fusions_executed,
            neg_fusions_executed: self.neg_fusions_executed,
            telemetry: self.cpu.telemetry,
            stdout_buffer: self.stdout_buffer.clone(),
        }
    }
}

/// Scans executable ranges in the ELF image to discover all static jump and branch targets.
fn scan_static_branch_targets(
    memory: &Memory,
    layout: &LoadedElfLayout,
    image: &Elf32Image,
) -> HashSet<u32> {
    let mut targets = HashSet::new();
    targets.insert(layout.entry_point);
    for &sym_addr in image.symbols.values() {
        targets.insert(sym_addr);
    }

    for &(start, end) in &layout.executable_ranges {
        let mut pc = start;
        while pc + 4 <= end {
            if let Ok(raw) = read_elf_u32_le(memory, layout.setun_base_offset, pc) {
                if let Ok(
                    Rv32iInstr::Jal { offset, .. }
                    | Rv32iInstr::Beq { offset, .. }
                    | Rv32iInstr::Bne { offset, .. }
                    | Rv32iInstr::Blt { offset, .. }
                    | Rv32iInstr::Bge { offset, .. }
                    | Rv32iInstr::Bltu { offset, .. }
                    | Rv32iInstr::Bgeu { offset, .. },
                ) = Rv32iInstr::decode(raw)
                {
                    let target = (pc as i32).wrapping_add(offset) as u32;
                    targets.insert(target);
                }
            }
            pc = pc.wrapping_add(4);
        }
    }
    targets
}

/// Maps an `RV32I` instruction to the corresponding [`Opcode`] in Setun 21-trit ISA.
fn map_rv32i_to_setun_opcode(instr: &Rv32iInstr) -> Opcode {
    match instr {
        Rv32iInstr::Lui { .. } | Rv32iInstr::Auipc { .. } => Opcode::Load,
        Rv32iInstr::Jal { .. } | Rv32iInstr::Jalr { .. } => Opcode::Call,
        Rv32iInstr::Beq { .. } | Rv32iInstr::Bne { .. } => Opcode::JmpZ,
        Rv32iInstr::Blt { .. } | Rv32iInstr::Bltu { .. } => Opcode::JmpN,
        Rv32iInstr::Bge { .. } | Rv32iInstr::Bgeu { .. } => Opcode::JmpP,
        Rv32iInstr::Lb { .. }
        | Rv32iInstr::Lbu { .. }
        | Rv32iInstr::Lh { .. }
        | Rv32iInstr::Lhu { .. }
        | Rv32iInstr::Lw { .. } => Opcode::Load,
        Rv32iInstr::Sb { .. } | Rv32iInstr::Sh { .. } | Rv32iInstr::Sw { .. } => Opcode::Store,
        Rv32iInstr::Addi { .. } | Rv32iInstr::Add { .. } => Opcode::Add,
        Rv32iInstr::Sub { .. } => Opcode::Sub,
        Rv32iInstr::Slti { .. }
        | Rv32iInstr::Sltiu { .. }
        | Rv32iInstr::Slt { .. }
        | Rv32iInstr::Sltu { .. } => Opcode::CmpT,
        Rv32iInstr::Slli { .. }
        | Rv32iInstr::Sll { .. }
        | Rv32iInstr::Srli { .. }
        | Rv32iInstr::Srai { .. }
        | Rv32iInstr::Srl { .. }
        | Rv32iInstr::Sra { .. } => Opcode::Shift,
        Rv32iInstr::Xori { .. } | Rv32iInstr::Xor { .. } => Opcode::Smt,
        Rv32iInstr::Ori { .. } | Rv32iInstr::Or { .. } => Opcode::Sat,
        Rv32iInstr::Andi { .. } | Rv32iInstr::And { .. } => Opcode::Sit,
        Rv32iInstr::Mul { .. }
        | Rv32iInstr::Mulh { .. }
        | Rv32iInstr::Mulhsu { .. }
        | Rv32iInstr::Mulhu { .. } => Opcode::Mul,
        Rv32iInstr::Div { .. }
        | Rv32iInstr::Divu { .. }
        | Rv32iInstr::Rem { .. }
        | Rv32iInstr::Remu { .. } => Opcode::Div,
        Rv32iInstr::Fence => Opcode::Nop,
        Rv32iInstr::Ecall | Rv32iInstr::Ebreak => Opcode::Halt,
    }
}

#[inline]
fn decode_s_imm(raw: u32) -> i32 {
    let imm4_0 = (raw >> 7) & 0x1F;
    let imm11_5 = (raw >> 25) & 0x7F;
    let imm12 = (imm11_5 << 5) | imm4_0;
    ((imm12 << 20) as i32) >> 20
}

#[inline]
fn decode_b_imm(raw: u32) -> i32 {
    let bit11 = (raw >> 7) & 0x01;
    let bits4_1 = (raw >> 8) & 0x0F;
    let bits10_5 = (raw >> 25) & 0x3F;
    let bit12 = (raw >> 31) & 0x01;
    let imm13 = (bit12 << 12) | (bit11 << 11) | (bits10_5 << 5) | (bits4_1 << 1);
    ((imm13 << 19) as i32) >> 19
}

#[inline]
fn decode_j_imm(raw: u32) -> i32 {
    let bits19_12 = (raw >> 12) & 0xFF;
    let bit11 = (raw >> 20) & 0x01;
    let bits10_1 = (raw >> 21) & 0x3FF;
    let bit20 = (raw >> 31) & 0x01;
    let imm21 = (bit20 << 20) | (bits19_12 << 12) | (bit11 << 11) | (bits10_1 << 1);
    ((imm21 << 11) as i32) >> 11
}

/// Consolidated execution summary of the entire official 37 `RV32I` microtest suite on Setun-Doom.
#[derive(Clone, Debug)]
pub struct RiscvIsaSuiteSummary {
    /// Individual results for the 37 tests (`(test_name, DbtExecutionReport)`).
    pub tests: Vec<(&'static str, DbtExecutionReport)>,
    /// Total equivalent binary `RV32I` instructions in the suite.
    pub total_rv32i_instrs: u64,
    /// Total ternary `Setun-Doom` instructions executed in the suite.
    pub total_setun_instrs: u64,
    /// Total `LI.T` and `AIPC.T` fusions executed.
    pub total_immediate_fusions: u64,
    /// Total trichotomous `BCEG` fusions executed.
    pub total_branch_fusions: u64,
    /// Total Setun `NEG/STI.T` sign inversions executed.
    pub total_neg_fusions: u64,
    /// Accumulated telemetry across all 37 tests.
    pub total_telemetry: PowerTelemetry,
}

impl RiscvIsaSuiteSummary {
    /// Percentage difference of instructions executed across the entire suite (`Setun` vs `RV32I`).
    pub fn instruction_diff_pct(&self) -> f64 {
        if self.total_rv32i_instrs == 0 {
            return 0.0;
        }
        let r = self.total_rv32i_instrs as f64;
        let s = self.total_setun_instrs as f64;
        ((s - r) / r) * 100.0
    }
}

/// Static list of 37 official tests from the `RV32I` specification compiled into `ELF32`.
pub const RISCV_ISA_ELF_TESTS: [(&str, &[u8]); 37] = [
    ("add", include_bytes!("../vendor/riscv_tests/elf/add.elf")),
    ("addi", include_bytes!("../vendor/riscv_tests/elf/addi.elf")),
    ("and", include_bytes!("../vendor/riscv_tests/elf/and.elf")),
    ("andi", include_bytes!("../vendor/riscv_tests/elf/andi.elf")),
    ("auipc", include_bytes!("../vendor/riscv_tests/elf/auipc.elf")),
    ("beq", include_bytes!("../vendor/riscv_tests/elf/beq.elf")),
    ("bge", include_bytes!("../vendor/riscv_tests/elf/bge.elf")),
    ("bgeu", include_bytes!("../vendor/riscv_tests/elf/bgeu.elf")),
    ("blt", include_bytes!("../vendor/riscv_tests/elf/blt.elf")),
    ("bltu", include_bytes!("../vendor/riscv_tests/elf/bltu.elf")),
    ("bne", include_bytes!("../vendor/riscv_tests/elf/bne.elf")),
    ("jal", include_bytes!("../vendor/riscv_tests/elf/jal.elf")),
    ("jalr", include_bytes!("../vendor/riscv_tests/elf/jalr.elf")),
    ("lb", include_bytes!("../vendor/riscv_tests/elf/lb.elf")),
    ("lbu", include_bytes!("../vendor/riscv_tests/elf/lbu.elf")),
    ("lh", include_bytes!("../vendor/riscv_tests/elf/lh.elf")),
    ("lhu", include_bytes!("../vendor/riscv_tests/elf/lhu.elf")),
    ("lui", include_bytes!("../vendor/riscv_tests/elf/lui.elf")),
    ("lw", include_bytes!("../vendor/riscv_tests/elf/lw.elf")),
    ("or", include_bytes!("../vendor/riscv_tests/elf/or.elf")),
    ("ori", include_bytes!("../vendor/riscv_tests/elf/ori.elf")),
    ("sb", include_bytes!("../vendor/riscv_tests/elf/sb.elf")),
    ("sh", include_bytes!("../vendor/riscv_tests/elf/sh.elf")),
    ("sll", include_bytes!("../vendor/riscv_tests/elf/sll.elf")),
    ("slli", include_bytes!("../vendor/riscv_tests/elf/slli.elf")),
    ("slt", include_bytes!("../vendor/riscv_tests/elf/slt.elf")),
    ("slti", include_bytes!("../vendor/riscv_tests/elf/slti.elf")),
    ("sltiu", include_bytes!("../vendor/riscv_tests/elf/sltiu.elf")),
    ("sltu", include_bytes!("../vendor/riscv_tests/elf/sltu.elf")),
    ("sra", include_bytes!("../vendor/riscv_tests/elf/sra.elf")),
    ("srai", include_bytes!("../vendor/riscv_tests/elf/srai.elf")),
    ("srl", include_bytes!("../vendor/riscv_tests/elf/srl.elf")),
    ("srli", include_bytes!("../vendor/riscv_tests/elf/srli.elf")),
    ("sub", include_bytes!("../vendor/riscv_tests/elf/sub.elf")),
    ("sw", include_bytes!("../vendor/riscv_tests/elf/sw.elf")),
    ("xor", include_bytes!("../vendor/riscv_tests/elf/xor.elf")),
    ("xori", include_bytes!("../vendor/riscv_tests/elf/xori.elf")),
];

/// Executes all 37 official `RV32I` ISA tests on `SetunDbtEngine`.
pub fn run_all_riscv_isa_tests() -> Result<RiscvIsaSuiteSummary, DbtError> {
    let mut results = Vec::with_capacity(RISCV_ISA_ELF_TESTS.len());
    let mut total_rv32i_instrs = 0u64;
    let mut total_setun_instrs = 0u64;
    let mut total_immediate_fusions = 0u64;
    let mut total_branch_fusions = 0u64;
    let mut total_neg_fusions = 0u64;
    let mut total_telemetry = PowerTelemetry::default();

    for &(name, elf_bytes) in &RISCV_ISA_ELF_TESTS {
        let mut engine = SetunDbtEngine::from_elf_bytes(elf_bytes)?;
        let report = engine.run(100_000)?;
        total_rv32i_instrs += report.rv32i_instructions_executed;
        total_setun_instrs += report.setun_instructions_executed;
        total_immediate_fusions += report.immediate_fusions_executed;
        total_branch_fusions += report.branch_fusions_executed;
        total_neg_fusions += report.neg_fusions_executed;

        total_telemetry.ternary_trit_flips += report.telemetry.ternary_trit_flips;
        total_telemetry.ternary_half_swings += report.telemetry.ternary_half_swings;
        total_telemetry.ternary_full_swings += report.telemetry.ternary_full_swings;
        total_telemetry.binary32_bit_flips += report.telemetry.binary32_bit_flips;
        total_telemetry.adder_carry_propagations += report.telemetry.adder_carry_propagations;

        results.push((name, report));
    }

    Ok(RiscvIsaSuiteSummary {
        tests: results,
        total_rv32i_instrs,
        total_setun_instrs,
        total_immediate_fusions,
        total_branch_fusions,
        total_neg_fusions,
        total_telemetry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_37_riscv_rv32i_official_isa_tests_on_setun_dbt() {
        let summary =
            run_all_riscv_isa_tests().expect("Failed to execute 37-test RV32I suite");

        assert_eq!(summary.tests.len(), 37);

        for (name, report) in &summary.tests {
            assert!(
                report.is_riscv_test_pass(),
                "Official RV32I test '{}' failed: {:?}",
                name,
                report.exit
            );
            assert!(
                report.telemetry.adder_carry_propagations > 0,
                "Test '{}' did not record carry propagations on ternary gates",
                name
            );
        }

        // Verify that immediate (LI.T / AIPC.T) and trichotomous branch (BCEG) fusions
        // reduced the executed instruction count compared to binary RV32I.
        assert!(summary.total_immediate_fusions > 0);
        assert!(summary.total_branch_fusions > 0);
        assert!(summary.total_setun_instrs < summary.total_rv32i_instrs);
        assert!(
            summary.total_telemetry.ternary_trit_flips < summary.total_telemetry.binary32_bit_flips
        );
    }

    #[test]
    fn test_rv32m_extension_execution_on_setun_dbt() {
        let elf_bytes = include_bytes!("../vendor/riscv_tests/rv32m/rv32m_test.elf");
        let mut engine = SetunDbtEngine::from_elf_bytes(elf_bytes)
            .expect("Failed to load RV32M test ELF");
        let report = engine
            .run(50_000)
            .expect("Execution error in RV32M test");
        assert!(
            report.is_riscv_test_pass(),
            "RV32M extension test failed with exit: {:?}",
            report.exit
        );
    }

    #[test]
    fn test_rv32im_doom_benchmark_execution_on_setun_dbt() {
        let elf_bytes = include_bytes!("../vendor/riscv_tests/rv32m/doom_rv32_benchmark.elf");
        let mut engine = SetunDbtEngine::from_elf_bytes(elf_bytes)
            .expect("Failed to load Doom RV32IM benchmark ELF");
        let report = engine
            .run(5_000_000)
            .expect("Execution error in Doom RV32IM benchmark");
        assert!(
            report.is_riscv_test_pass(),
            "Doom RV32IM benchmark failed with exit: {:?}",
            report.exit
        );
        let stdout_str = String::from_utf8_lossy(&report.stdout_buffer);
        assert!(stdout_str.contains("Starting Doom RV32IM Renderer"));
        assert!(stdout_str.contains("Doom RV32IM Rendering Successfully Completed!"));
    }
}
