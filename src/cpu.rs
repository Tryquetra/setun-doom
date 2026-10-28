//! Central Processing Unit (`Setun-Doom CPU`), Instruction Set Architecture (ISA),
//! and Fetch-Decode-Execute Pipeline.
//!
//! 21-trit Instruction Layout (`Word21`):
//! - **Trits `[0..13]` (14 trits)**: Immediate operand or address ($3^{14} = 4,782,969$ locations).
//! - **Trits `[14..15]` (2 trits)**: Target index register ($3^2 = 9$ registers, encoded
//!   in `[-4..+4]` or mapped to `0..8`).
//! - **Trits `[16..20]` (5 trits)**: Opcode ($3^5 = 243$ possible instructions, in `[-121..+121]`).
//!
//! Trichotomous Conditional Branches:
//! The Setun architecture dispenses with binary status flag registers (Zero/Sign/Carry/Overflow). Branches
//! directly inspect the intrinsic ternary state of the accumulator (`ACC.sign_trit()`):
//! - `JMPN`: branches if `ACC < 0` (`Trit::Negative`).
//! - `JMPZ`: branches if `ACC == 0` (`Trit::Zero`).
//! - `JMPP`: branches if `ACC > 0` (`Trit::Positive`).
//! - `JMP`: unconditional branch.

use crate::fixed::TernaryFixed;
use crate::memory::{Memory, INITIAL_STACK_POINTER, MEMORY_SIZE};
use crate::trit::Trit;
use crate::word::{extract_trit_field, Word21, POW3};

/// Number of index registers in the Setun-Doom architecture ($3^2 = 9$).
pub const INDEX_REGISTER_COUNT: usize = 9;

/// 5-trit opcodes of the Setun-Doom architecture (`[-121, +121]`).
#[repr(i8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Opcode {
    /// No operation (`0`).
    Nop = 0,
    /// `ACC = ACC + MEM[operand]` (`+1`).
    Add = 1,
    /// `ACC = ACC - MEM[operand]` (`-1`).
    Sub = -1,
    /// `ACC = FixedMul(ACC, MEM[operand])` in 11.10 balanced ternary fixed point.
    /// By default (`reg_idx == 0`, Setun-58 `++0` style), copies previous multiplicand `ACC` to `R`;
    /// if `reg_idx == 1`, deposits 10-trit fractional remainder into `R` (`+2`).
    Mul = 2,
    /// `ACC = FixedDiv(ACC, MEM[operand])` in 11.10 balanced ternary fixed point (`-2`).
    Div = -2,
    /// `ACC = -ACC` (instantaneous inversion of all 21 trits) (`+3`).
    Neg = 3,

    /// `ACC = MEM[operand]` (`+4`).
    Load = 4,
    /// `MEM[operand] = ACC` (`-4`).
    Store = -4,
    /// `ACC = MEM[operand + IX[reg]]` (`+5`).
    LoadIndex = 5,
    /// `MEM[operand + IX[reg]] = ACC` (`-5`).
    StoreIndex = -5,

    /// Unconditional jump: `PC = operand` (`+6`).
    Jmp = 6,
    /// Trichotomous negative jump (`reg_idx == 0`: jumps if `ACC < 0`; `reg_idx == 1`: jumps if `ACC >= 0` / `JMP_GE`) (`-7`).
    JmpN = -7,
    /// Trichotomous zero jump (`reg_idx == 0`: jumps if `ACC == 0`; `reg_idx == 1`: jumps if `ACC != 0` / `JMP_NE`) (`+7`).
    JmpZ = 7,
    /// Trichotomous positive jump (`reg_idx == 0`: jumps if `ACC > 0`; `reg_idx == 1`: jumps if `ACC <= 0` / `JMP_LE`) (`+8`).
    JmpP = 8,
    /// Branch Compare Equal or Greater (`bceg` from REBEL-2):
    /// Compares `ACC` with `IX[reg]` (or uses direct `ACC`) and dispatches 3 ways in a single cycle
    /// using two 7-trit relative offsets packed inside the 14-trit operand:
    /// - If `ACC == IX[reg]`: `PC = current_PC + offset_eq` (trits `0..6`, `-1093..+1093`)
    /// - If `ACC > IX[reg]`:  `PC = current_PC + offset_gt` (trits `7..13`, `-1093..+1093`)
    /// - If `ACC < IX[reg]`:  falls through to `PC + 1` (`+19`).
    Bceg = 19,

    /// Push `ACC`: `MEM[SP] = ACC; SP -= 1` (`+9`).
    Push = 9,
    /// Pop into `ACC`: `SP += 1; ACC = MEM[SP]` (`-9`).
    Pop = -9,
    /// Subroutine call: pushes return `PC` and jumps to `operand` (`+10`).
    Call = 10,
    /// Subroutine return: pops address and restores `PC` (`-10`).
    Ret = -10,

    /// Loads 14-trit immediate constant into `ACC`: `ACC = operand` (`+11`).
    LoadImm = 11,
    /// Adds 14-trit immediate constant to `ACC`: `ACC = ACC + operand` (`-11`).
    AddImm = -11,
    /// Loads 14-trit immediate constant into index register `IX[reg]` (`+12`).
    LoadIxImm = 12,
    /// Adds immediate constant to index register `IX[reg]` (`-12`).
    AddIxImm = -12,
    /// Copies `ACC` to `IX[reg]` (if `reg == 8`, also updates multiplier register `R`) (`+13`).
    MovAccToIx = 13,
    /// Copies `IX[reg]` to `ACC` (if `reg == 8`, reads multiplier register `R`) (`-13`).
    MovIxToAcc = -13,
    /// Ternary trit shift by `operand` positions:
    /// if `reg_idx == 0`, shifts `ACC`; if `reg_idx > 0`, shifts `IX[reg_idx]` and stores in `ACC` (`+14`).
    Shift = 14,
    /// Pure integer multiplication: `ACC = ACC * MEM[operand]` (`-14`).
    MulInt = -14,
    /// Pure integer division: `ACC = ACC / MEM[operand]` (`+15`).
    DivInt = 15,
    /// Trichotomous Comparison (`cmp.t` from REBEL-6 paper): `ACC = ACC <=> MEM[operand]` in `{-1, 0, +1}` (`+16`).
    CmpT = 16,
    /// Ternary Majority Vote (`majv.t` from REBEL-6 paper):
    /// `ACC = MajorityVote(ACC, IX[reg], MEM[operand])` trit-by-trit (`+17`).
    MajvT = 17,
    /// Ternary Minority Vote (`minv.t` from REBEL-6 paper):
    /// `ACC = MinorityVote(ACC, IX[reg], MEM[operand])` trit-by-trit (`-17`).
    MinvT = -17,
    /// Adds memory word directly to register `IX[reg]`:
    /// `IX[reg] = IX[reg] + MEM[operand]` without clobbering `ACC` (`+20`).
    AddIx = 20,
    /// Decrements `IX[reg]` by 1 and jumps to `operand` if `IX[reg] >= 0` (`BIX` / index loop
    /// Setun style without overwriting `ACC`) (`-20`).
    DecIxJmpGe = -20,
    /// `R = MEM[operand]` (`PARCEL_TO_R` `+-+` from Setun-58 / `YRT` from Setun-70) (`+22`).
    LoadR = 22,
    /// `ACC = ACC + FixedMul(MEM[operand], R)` (`MULTIPLICATION_POS` `+++` from Setun-58 / `LBT` `B25` from Setun-70) (`+24`).
    MacPos = 24,
    /// `ACC = ACC - FixedMul(MEM[operand], R)` (`MULTIPLICATION_NEG` `++-` from Setun-58) (`-24`).
    MacNeg = -24,
    /// Trit-by-trit multiplication `ACC = ACC (*) MEM[operand]` (`SMT` `+-0` from Setun-58 / `B19` from Setun-70) (`+25`).
    Smt = 25,
    /// Trit-by-trit threshold addition `ACC = ThresholdAdd(ACC, MEM[operand])` (`SAT` `B21` `000+-+` from Setun-70) (`-25`).
    Sat = -25,
    /// Trit-by-trit masked inhibition/coalescence `ACC = ACC + MEM[operand] * (1 - |ACC|)` (`SIT` `B20` `000+-0` from Setun-70) (`+26`).
    Sit = 26,
    /// Mantissa normalization and leading zero count:
    /// `(ACC, IX[reg]) = ACC.normalize_leading()` (`NORM.T` / `CLZ.T`) (`+27`).
    NormT = 27,
    /// Bitwise AND: `ACC = ACC & MEM[operand]` (`+28`).
    And = 28,
    /// Bitwise OR: `ACC = ACC | MEM[operand]` (`+29`).
    Or = 29,
    /// Bitwise XOR: `ACC = ACC ^ MEM[operand]` (`+30`).
    Xor = 30,
    /// Controlled CPU halt (`HALT`) (`+40`).
    Halt = 40,
    /// Software interrupt / system call trap (`SYS` / `TRAP`) (`+41`).
    Syscall = 41,
}

impl Opcode {
    /// Decodes a 5-trit field (`-121..=+121`) into an `Opcode`.
    #[inline(always)]
    pub const fn from_trit_field(val: i64) -> Option<Self> {
        match val {
            0 => Some(Self::Nop),
            1 => Some(Self::Add),
            -1 => Some(Self::Sub),
            2 => Some(Self::Mul),
            -2 => Some(Self::Div),
            3 => Some(Self::Neg),
            4 => Some(Self::Load),
            -4 => Some(Self::Store),
            5 => Some(Self::LoadIndex),
            -5 => Some(Self::StoreIndex),
            6 => Some(Self::Jmp),
            -7 => Some(Self::JmpN),
            7 => Some(Self::JmpZ),
            8 => Some(Self::JmpP),
            9 => Some(Self::Push),
            -9 => Some(Self::Pop),
            10 => Some(Self::Call),
            -10 => Some(Self::Ret),
            11 => Some(Self::LoadImm),
            -11 => Some(Self::AddImm),
            12 => Some(Self::LoadIxImm),
            -12 => Some(Self::AddIxImm),
            13 => Some(Self::MovAccToIx),
            -13 => Some(Self::MovIxToAcc),
            14 => Some(Self::Shift),
            -14 => Some(Self::MulInt),
            15 => Some(Self::DivInt),
            16 => Some(Self::CmpT),
            17 => Some(Self::MajvT),
            -17 => Some(Self::MinvT),
            19 => Some(Self::Bceg),
            20 => Some(Self::AddIx),
            -20 => Some(Self::DecIxJmpGe),
            22 => Some(Self::LoadR),
            24 => Some(Self::MacPos),
            -24 => Some(Self::MacNeg),
            25 => Some(Self::Smt),
            -25 => Some(Self::Sat),
            26 => Some(Self::Sit),
            27 => Some(Self::NormT),
            28 => Some(Self::And),
            29 => Some(Self::Or),
            30 => Some(Self::Xor),
            40 => Some(Self::Halt),
            41 => Some(Self::Syscall),
            _ => None,
        }
    }
}

/// Decoded instruction of Setun-Doom architecture.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// 5-trit Opcode.
    pub opcode: Opcode,
    /// Target register index (`0..=8`, mapped from 2 trits `[-4..+4]`).
    pub reg_idx: usize,
    /// 14-trit immediate operand or address (`[-2_391_484, +2_391_484]`).
    pub operand: i64,
}

impl Instruction {
    /// Creates a new instruction validating register index (`0..9`).
    #[inline(always)]
    #[must_use]
    pub const fn new(opcode: Opcode, reg_idx: usize, operand: i64) -> Self {
        assert!(reg_idx < INDEX_REGISTER_COUNT);
        Self {
            opcode,
            reg_idx,
            operand,
        }
    }

    /// Encodes instruction into a 21-trit balanced word:
    /// `word = operand + (reg_trits * 3^14) + (opcode * 3^16)`.
    #[inline(always)]
    pub const fn encode(self) -> Word21 {
        let reg_balanced = (self.reg_idx as i64) - 4; // 0..8 -> -4..+4 (2 trits)
        let op_balanced = self.opcode as i64; // 5 trits
                                              // Ensures operand stays in 14-trit balanced range [-2_391_484, +2_391_484]
        let operand_balanced = crate::word::wrap_balanced::<14>(self.operand);
        let raw = operand_balanced + reg_balanced * POW3[14] + op_balanced * POW3[16];
        Word21::new(raw)
    }

    /// Decodes a `Word21` into `Instruction` extracting 14, 2, and 5-trit fields.
    #[inline(always)]
    pub const fn decode(word: Word21) -> Option<Self> {
        let raw = word.as_i64();
        let operand = extract_trit_field(raw, 0, 14);
        let reg_balanced = extract_trit_field(raw, 14, 2); // -4..+4
        let op_balanced = extract_trit_field(raw, 16, 5); // -121..+121

        let opcode = match Opcode::from_trit_field(op_balanced) {
            Some(op) => op,
            None => return None,
        };
        let reg_idx = (reg_balanced + 4) as usize;
        Some(Self {
            opcode,
            reg_idx,
            operand,
        })
    }

    /// Converts 14-trit balanced operand to linear address `0..4_782_969`
    /// when used for high-memory addressing (such as VGA Framebuffer).
    #[inline(always)]
    pub const fn operand_as_address(self) -> i64 {
        if self.operand < 0 {
            self.operand + (MEMORY_SIZE as i64)
        } else {
            self.operand
        }
    }

    /// Packs two 7-trit relative offsets (`offset_eq` and `offset_gt`, each in `[-1093..+1093]`)
    /// into a 14-trit operand for the `BCEG` instruction.
    #[inline(always)]
    pub const fn pack_bceg_offsets(offset_eq: i64, offset_gt: i64) -> i64 {
        let eq7 = crate::word::wrap_balanced::<7>(offset_eq);
        let gt7 = crate::word::wrap_balanced::<7>(offset_gt);
        eq7 + gt7 * POW3[7]
    }

    /// Unpacks two 7-trit relative offsets (`(offset_eq, offset_gt)`) from a `BCEG` operand.
    #[inline(always)]
    pub const fn unpack_bceg_offsets(operand: i64) -> (i64, i64) {
        let eq7 = extract_trit_field(operand, 0, 7);
        let gt7 = extract_trit_field(operand, 7, 7);
        (eq7, gt7)
    }

    /// Constructs a `BCEG` instruction comparing `ACC` with `IX[reg_idx]`.
    #[inline(always)]
    #[must_use]
    pub const fn new_bceg(reg_idx: usize, offset_eq: i64, offset_gt: i64) -> Self {
        Self::new(
            Opcode::Bceg,
            reg_idx,
            Self::pack_bceg_offsets(offset_eq, offset_gt),
        )
    }
}

/// Hardware Traps and Exceptions for Setun-Doom CPU (`CpuTrap`).
/// Prevents silent failures (such as invalid decoding treated as `HALT`,
/// ignored division by zero, or memory access outside 14-trit address space).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CpuTrap {
    /// Instruction word contains invalid opcode or register index.
    InvalidInstruction { pc: i64, raw_word: Word21 },
    /// Attempted division by zero in `Opcode::Div` or `Opcode::DivInt`.
    DivisionByZero { pc: i64, opcode: Opcode },
    /// Boundary violation on 14-trit memory bus.
    MemoryFault {
        pc: i64,
        error: crate::memory::MemoryError,
    },
    /// Software system call or trap request (`Opcode::Syscall`).
    Syscall { pc: i64, num: i64 },
}

impl core::fmt::Display for CpuTrap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidInstruction { pc, raw_word } => {
                write!(f, "Invalid instruction at PC {pc}: {raw_word}")
            }
            Self::DivisionByZero { pc, opcode } => {
                write!(f, "Division by zero at PC {pc} in {opcode:?}")
            }
            Self::MemoryFault { pc, error } => {
                write!(f, "Memory fault at PC {pc}: {error}")
            }
            Self::Syscall { pc, num } => {
                write!(f, "Syscall trap at PC {pc} with syscall number {num}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CpuTrap {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MemoryFault { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Power efficiency and state switching telemetry (*Digit Flips*) based on
/// Section IV.B methodology from **REBEL-6** paper (IEEE ISMVL 2025).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerTelemetry {
    /// Total trits that changed state when updating registers (`Word21`).
    pub ternary_trit_flips: u64,
    /// Count of half-swing ternary transitions (`-1 <-> 0` or `0 <-> +1`).
    pub ternary_half_swings: u64,
    /// Count of full-swing ternary transitions (`-1 <-> +1`).
    pub ternary_full_swings: u64,
    /// Total bits that would toggle in 32-bit binary registers (`RV32I` two's complement)
    /// executing equivalent operations in native base representations.
    pub binary32_bit_flips: u64,
    /// Active carry propagations measured across `Trit::full_add` stages in ternary ALU.
    pub adder_carry_propagations: u64,
}

impl PowerTelemetry {
    /// Records an integer register write from `prev` to `next`.
    #[inline(always)]
    pub fn record_transition(&mut self, prev: Word21, next: Word21) {
        if prev == next {
            return;
        }
        let (t_flips, half, full) = prev.count_trit_flips(next);
        let b_flips = prev.count_binary32_flips(next);
        self.ternary_trit_flips += t_flips as u64;
        self.ternary_half_swings += half as u64;
        self.ternary_full_swings += full as u64;
        self.binary32_bit_flips += b_flips as u64;
    }

    /// Records a 1-to-1 paired fixed-point transition comparing
    /// `prev_ternary -> next_ternary` in `Word21` (`11.10`, scale $3^{10} = 59,049$)
    /// against corresponding exact transition `prev_binary16 -> next_binary16` in `RV32I` (`16.16`, scale $2^{16} = 65,536$).
    #[inline(always)]
    pub fn record_fixed16_transition(
        &mut self,
        prev_ternary: Word21,
        next_ternary: Word21,
        prev_binary16: i32,
        next_binary16: i32,
    ) {
        if prev_ternary != next_ternary {
            let (t_flips, half, full) = prev_ternary.count_trit_flips(next_ternary);
            self.ternary_trit_flips += t_flips as u64;
            self.ternary_half_swings += half as u64;
            self.ternary_full_swings += full as u64;
        }
        if prev_binary16 != next_binary16 {
            let b_flips = (prev_binary16 ^ next_binary16).count_ones();
            self.binary32_bit_flips += b_flips as u64;
        }
    }

    /// Computes percentage reduction of *Digit Flips* for ternary architecture versus 32-bit binary
    /// (e.g. `33.24` indicates `33.24%` dynamic switching savings as reported in REBEL-6 paper).
    pub fn digit_flip_reduction_pct(&self) -> f64 {
        if self.binary32_bit_flips == 0 {
            return 0.0;
        }
        let b = self.binary32_bit_flips as f64;
        let t = self.ternary_trit_flips as f64;
        ((b - t) / b) * 100.0
    }

    /// Computes percentage reduction in dynamic capacitive switching energy ($E \propto \Delta V^2$),
    /// weighting each *Half-Swing* (`-1 <-> 0`, `0 <-> +1`) with $0.25\times$ energy of a
    /// *Full-Swing* (`-1 <-> +1` or binary `0 <-> 1` toggle).
    pub fn weighted_energy_reduction_pct(&self) -> f64 {
        if self.binary32_bit_flips == 0 {
            return 0.0;
        }
        let b_energy = self.binary32_bit_flips as f64;
        let t_energy =
            (self.ternary_half_swings as f64) * 0.25 + (self.ternary_full_swings as f64) * 1.0;
        ((b_energy - t_energy) / b_energy) * 100.0
    }

    /// Returns [`Self::weighted_energy_reduction_pct`] only when a binary reference counter was
    /// recorded, so callers report `None` instead of a placeholder percentage.
    pub fn weighted_energy_reduction_opt(&self) -> Option<f64> {
        if self.binary32_bit_flips == 0 {
            None
        } else {
            Some(self.weighted_energy_reduction_pct())
        }
    }
}

/// Full architectural state of Setun-Doom CPU.
pub struct Cpu {
    /// 21-trit main accumulator (`ACC`).
    pub acc: Word21,
    /// 21-trit multiplier/quotient register (`R`).
    pub r: Word21,
    /// Program Counter (`PC`) pointing to address of next instruction.
    pub pc: i64,
    /// Stack Pointer (`SP`).
    pub sp: i64,
    /// Bank of 9 index registers (`IX[0..9]`), addressed by 2 trits (`-4..+4`).
    pub index_regs: [Word21; INDEX_REGISTER_COUNT],
    /// Indicates whether CPU encountered a legitimate `HALT` instruction.
    pub halted: bool,
    /// Hardware trap raised if execution error occurs (silent failure prohibited).
    pub trap: Option<CpuTrap>,
    /// Total count of executed cycles/instructions.
    pub cycles: u64,
    /// Comparative state switching telemetry (*Trit Flips* vs binary *Bit Flips*).
    pub telemetry: PowerTelemetry,
}

impl Cpu {
    /// Initializes CPU with zeroed registers and `SP` at top of RAM.
    #[must_use]
    pub fn new() -> Self {
        Self {
            acc: Word21::ZERO,
            r: Word21::ZERO,
            pc: 0,
            sp: INITIAL_STACK_POINTER,
            index_regs: [Word21::ZERO; INDEX_REGISTER_COUNT],
            halted: false,
            trap: None,
            cycles: 0,
            telemetry: PowerTelemetry::default(),
        }
    }

    /// Resets CPU state setting entry point `entry_pc`.
    pub fn reset(&mut self, entry_pc: i64) {
        self.acc = Word21::ZERO;
        self.r = Word21::ZERO;
        self.pc = entry_pc;
        self.sp = INITIAL_STACK_POINTER;
        self.index_regs = [Word21::ZERO; INDEX_REGISTER_COUNT];
        self.halted = false;
        self.trap = None;
        self.cycles = 0;
        self.telemetry = PowerTelemetry::default();
    }

    #[inline(always)]
    fn set_acc(&mut self, next: Word21) {
        self.telemetry.record_transition(self.acc, next);
        self.acc = next;
    }

    #[inline(always)]
    fn set_r(&mut self, next: Word21) {
        self.telemetry.record_transition(self.r, next);
        self.r = next;
    }

    #[inline(always)]
    fn set_ix(&mut self, idx: usize, next: Word21) {
        let prev = self.index_regs[idx];
        self.telemetry.record_transition(prev, next);
        self.index_regs[idx] = next;
    }

    /// Loads a slice of encoded instructions into memory starting at `base_addr`
    /// and positions `PC` at `base_addr`.
    pub fn load_program(&mut self, mem: &mut Memory, base_addr: i64, instructions: &[Instruction]) {
        for (offset, instr) in instructions.iter().enumerate() {
            mem.write_word(base_addr + (offset as i64), instr.encode())
                .expect("Failed to load program into ternary memory");
        }
        self.pc = base_addr;
        self.halted = false;
        self.trap = None;
    }

    /// Executes a single Fetch-Decode-Execute cycle with strict error verification (`CpuTrap`).
    /// Returns `Ok(true)` if instruction executed and CPU continues running, `Ok(false)` on `HALT`,
    /// or `Err(CpuTrap)` on invalid instruction, division by zero, or memory violation.
    #[inline]
    pub fn step_checked(&mut self, mem: &mut Memory) -> Result<bool, CpuTrap> {
        if let Some(trap) = self.trap {
            return Err(trap);
        }
        if self.halted {
            return Ok(false);
        }

        let instr_pc = self.pc;

        // 1. FETCH (validated within 14 trits)
        let instr_word = mem.read_word(instr_pc).map_err(|error| {
            let t = CpuTrap::MemoryFault {
                pc: instr_pc,
                error,
            };
            self.trap = Some(t);
            t
        })?;
        self.pc += 1;

        // 2. DECODE (treating invalid opcode as silent HALT is prohibited)
        let Some(instr) = Instruction::decode(instr_word) else {
            let t = CpuTrap::InvalidInstruction {
                pc: instr_pc,
                raw_word: instr_word,
            };
            self.trap = Some(t);
            return Err(t);
        };

        // Helper for checked reading and writing of operands
        let read_mem = |mem: &Memory, addr: i64| -> Result<Word21, CpuTrap> {
            mem.read_word(addr).map_err(|error| CpuTrap::MemoryFault {
                pc: instr_pc,
                error,
            })
        };
        let write_mem = |mem: &mut Memory, addr: i64, val: Word21| -> Result<(), CpuTrap> {
            mem.write_word(addr, val)
                .map_err(|error| CpuTrap::MemoryFault {
                    pc: instr_pc,
                    error,
                })
        };

        // 3. EXECUTE
        let exec_res: Result<bool, CpuTrap> = (|| {
            match instr.opcode {
                Opcode::Nop => {}

                Opcode::Add => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let (sum, _cout, carries) = self.acc.gate_ripple_carry_add(val, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(sum);
                }

                Opcode::Sub => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let neg_val = val.gate_invert_trits();
                    let (diff, _cout, carries) =
                        self.acc.gate_ripple_carry_add(neg_val, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(diff);
                }

                Opcode::Mul => {
                    // Unifies MUL and MUL_ZERO (++0 of Setun-58 / L*T B26 of Setun-70):
                    // By default (`reg_idx == 0`), copies previous multiplicand `ACC` to `R`;
                    // if `reg_idx == 1`, deposits 10-trit fractional remainder in `R`.
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let prev_acc = self.acc;
                    let a = TernaryFixed::from_raw(prev_acc);
                    let b = TernaryFixed::from_raw(val);
                    let (_prod42, carries) = prev_acc.gate_shift_and_add_mul42(val);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    let (prod, rem) = a.fixed_mul_with_rem(b);

                    // Paired 1-to-1 comparison without base bias (prev_16 -> next_16):
                    let a16 = a.to_doom_fixed16();
                    let b16 = b.to_doom_fixed16();
                    let bin_prod16 = (((a16 as i64) * (b16 as i64)) >> 16)
                        .clamp(i32::MIN as i64, i32::MAX as i64)
                        as i32;
                    self.telemetry.record_fixed16_transition(
                        prev_acc,
                        prod.raw_word(),
                        a16,
                        bin_prod16,
                    );
                    self.acc = prod.raw_word();
                    if instr.reg_idx == 1 {
                        self.set_r(rem);
                    } else {
                        self.set_r(prev_acc);
                    }
                }

                Opcode::Div => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let a = TernaryFixed::from_raw(self.acc);
                    let b = TernaryFixed::from_raw(val);
                    let Some((quot, carries)) = a.gate_fixed_div(b) else {
                        return Err(CpuTrap::DivisionByZero {
                            pc: instr_pc,
                            opcode: Opcode::Div,
                        });
                    };
                    self.telemetry.adder_carry_propagations += carries as u64;
                    let a16 = a.to_doom_fixed16();
                    let b16 = b.to_doom_fixed16();
                    let bin_quot16 = if b16 != 0 {
                        (((a16 as i64) << 16) / (b16 as i64))
                            .clamp(i32::MIN as i64, i32::MAX as i64) as i32
                    } else {
                        quot.to_doom_fixed16()
                    };
                    self.telemetry.record_fixed16_transition(
                        self.acc,
                        quot.raw_word(),
                        a16,
                        bin_quot16,
                    );
                    self.acc = quot.raw_word();
                }

                Opcode::Neg => {
                    // Physical 21-gate `Trit::invert` inversion (`sti.t` from REBEL-6 paper)
                    self.set_acc(self.acc.gate_invert_trits());
                }

                Opcode::Load => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(val);
                }

                Opcode::Store => {
                    let addr = instr.operand_as_address();
                    write_mem(mem, addr, self.acc)?;
                }

                Opcode::LoadIndex => {
                    let base = instr.operand_as_address();
                    let offset = self.index_regs[instr.reg_idx].as_i64();
                    let val = read_mem(mem, base + offset)?;
                    self.set_acc(val);
                }

                Opcode::StoreIndex => {
                    let base = instr.operand_as_address();
                    let offset = self.index_regs[instr.reg_idx].as_i64();
                    write_mem(mem, base + offset, self.acc)?;
                }

                Opcode::Jmp => {
                    self.pc = instr.operand_as_address();
                }

                Opcode::JmpN => {
                    // reg_idx == 0: jumps if ACC < 0 (JMPN); reg_idx == 1: jumps if ACC >= 0 (JMP_GE)
                    let is_neg = self.acc.sign_trit() == Trit::Negative;
                    if is_neg ^ (instr.reg_idx == 1) {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::JmpZ => {
                    // reg_idx == 0: jumps if ACC == 0 (JMPZ); reg_idx == 1: jumps if ACC != 0 (JMP_NE)
                    let is_zero = self.acc.sign_trit() == Trit::Zero;
                    if is_zero ^ (instr.reg_idx == 1) {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::JmpP => {
                    // reg_idx == 0: jumps if ACC > 0 (JMPP); reg_idx == 1: jumps if ACC <= 0 (JMP_LE)
                    let is_pos = self.acc.sign_trit() == Trit::Positive;
                    if is_pos ^ (instr.reg_idx == 1) {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::Bceg => {
                    // BCEG instruction operating via 21-trit physical magnitude comparator (`gate_cmp_t`):
                    let rhs = self.index_regs[instr.reg_idx];
                    let cmp = self.acc.gate_cmp_t(rhs);
                    let (offset_eq, offset_gt) = Instruction::unpack_bceg_offsets(instr.operand);
                    match cmp {
                        Trit::Zero => {
                            if offset_eq != 0 {
                                self.pc = instr_pc + offset_eq;
                            }
                        }
                        Trit::Positive => {
                            if offset_gt != 0 {
                                self.pc = instr_pc + offset_gt;
                            }
                        }
                        Trit::Negative => {}
                    }
                }

                Opcode::Push => {
                    write_mem(mem, self.sp, self.acc)?;
                    self.sp -= 1;
                }

                Opcode::Pop => {
                    self.sp += 1;
                    let val = read_mem(mem, self.sp)?;
                    self.set_acc(val);
                }

                Opcode::Call => {
                    write_mem(mem, self.sp, Word21::new(self.pc))?;
                    self.sp -= 1;
                    self.pc = instr.operand_as_address();
                }

                Opcode::Ret => {
                    self.sp += 1;
                    let ret_addr = read_mem(mem, self.sp)?;
                    self.pc = ret_addr.as_i64();
                }

                Opcode::LoadImm => {
                    self.set_acc(Word21::new(instr.operand));
                }

                Opcode::AddImm => {
                    let (sum, _cout, carries) = self
                        .acc
                        .gate_ripple_carry_add(Word21::new(instr.operand), Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(sum);
                }

                Opcode::LoadIxImm => {
                    self.set_ix(instr.reg_idx, Word21::new(instr.operand));
                }

                Opcode::AddIxImm => {
                    let (next, _cout, carries) = self.index_regs[instr.reg_idx]
                        .gate_ripple_carry_add(Word21::new(instr.operand), Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_ix(instr.reg_idx, next);
                }

                Opcode::MovAccToIx => {
                    self.set_ix(instr.reg_idx, self.acc);
                    if instr.reg_idx == 8 {
                        self.set_r(self.acc);
                    }
                }

                Opcode::MovIxToAcc => {
                    if instr.reg_idx == 8 {
                        self.set_acc(self.r);
                    } else {
                        self.set_acc(self.index_regs[instr.reg_idx]);
                    }
                }

                Opcode::Shift => {
                    // reg_idx == 0: shifts ACC; reg_idx > 0: shifts IX[reg_idx] into ACC without clobbering IX
                    let src = if instr.reg_idx == 0 {
                        self.acc
                    } else {
                        self.index_regs[instr.reg_idx]
                    };
                    self.set_acc(src.shift_trits(instr.operand as i32));
                }

                Opcode::MulInt => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let (_prod42, carries) = self.acc.gate_shift_and_add_mul42(val);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_acc(self.acc * val);
                }

                Opcode::DivInt => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let Some((quot, rem, carries)) = self.acc.gate_non_restoring_div_rem(val)
                    else {
                        return Err(CpuTrap::DivisionByZero {
                            pc: instr_pc,
                            opcode: Opcode::DivInt,
                        });
                    };
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_r(rem);
                    self.set_acc(quot);
                }

                Opcode::CmpT => {
                    let addr = instr.operand_as_address();
                    let rhs = read_mem(mem, addr)?;
                    self.set_acc(self.acc.cmp_t(rhs));
                }

                Opcode::MajvT => {
                    let addr = instr.operand_as_address();
                    let third = read_mem(mem, addr)?;
                    let second = self.index_regs[instr.reg_idx];
                    self.set_acc(self.acc.tritwise_majority(second, third));
                }

                Opcode::MinvT => {
                    let addr = instr.operand_as_address();
                    let third = read_mem(mem, addr)?;
                    let second = self.index_regs[instr.reg_idx];
                    self.set_acc(self.acc.tritwise_minority(second, third));
                }

                Opcode::AddIx => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let (next, _cout, carries) =
                        self.index_regs[instr.reg_idx].gate_ripple_carry_add(val, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_ix(instr.reg_idx, next);
                }

                Opcode::DecIxJmpGe => {
                    let (next, _cout, carries) = self.index_regs[instr.reg_idx]
                        .gate_ripple_carry_add(Word21::NEG_ONE, Trit::Zero);
                    self.telemetry.adder_carry_propagations += carries as u64;
                    self.set_ix(instr.reg_idx, next);
                    if next.sign_trit() != Trit::Negative {
                        self.pc = instr.operand_as_address();
                    }
                }

                Opcode::LoadR => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_r(val);
                }

                Opcode::MacPos => {
                    // Setun-58 (+++) / Setun-70 LBT (B25): ACC = ACC + FixedMul(MEM[op], R)
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let m = TernaryFixed::from_raw(val);
                    let r = TernaryFixed::from_raw(self.r);
                    let (_prod42, mul_carries) = val.gate_shift_and_add_mul42(self.r);
                    let (prod, _rem) = m.fixed_mul_with_rem(r);
                    let (sum, _cout, add_carries) =
                        self.acc.gate_ripple_carry_add(prod.raw_word(), Trit::Zero);
                    self.telemetry.adder_carry_propagations += (mul_carries + add_carries) as u64;

                    let acc16 = TernaryFixed::from_raw(self.acc).to_doom_fixed16();
                    let m16 = m.to_doom_fixed16();
                    let r16 = r.to_doom_fixed16();
                    let bin_prod16 = (((m16 as i64) * (r16 as i64)) >> 16)
                        .clamp(i32::MIN as i64, i32::MAX as i64)
                        as i32;
                    let bin_mac16 = acc16.wrapping_add(bin_prod16);
                    self.telemetry
                        .record_fixed16_transition(self.acc, sum, acc16, bin_mac16);
                    self.acc = sum;
                }

                Opcode::MacNeg => {
                    // Setun-58 (++-): ACC = ACC - FixedMul(MEM[op], R)
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let m = TernaryFixed::from_raw(val);
                    let r = TernaryFixed::from_raw(self.r);
                    let (_prod42, mul_carries) = val.gate_shift_and_add_mul42(self.r);
                    let (prod, _rem) = m.fixed_mul_with_rem(r);
                    let neg_prod = prod.raw_word().gate_invert_trits();
                    let (diff, _cout, add_carries) =
                        self.acc.gate_ripple_carry_add(neg_prod, Trit::Zero);
                    self.telemetry.adder_carry_propagations += (mul_carries + add_carries) as u64;

                    let acc16 = TernaryFixed::from_raw(self.acc).to_doom_fixed16();
                    let m16 = m.to_doom_fixed16();
                    let r16 = r.to_doom_fixed16();
                    let bin_prod16 = (((m16 as i64) * (r16 as i64)) >> 16)
                        .clamp(i32::MIN as i64, i32::MAX as i64)
                        as i32;
                    let bin_mac16 = acc16.wrapping_sub(bin_prod16);
                    self.telemetry
                        .record_fixed16_transition(self.acc, diff, acc16, bin_mac16);
                    self.acc = diff;
                }

                Opcode::Smt => {
                    // Setun-58 (+-0) / Setun-70 B19 (SMT): trit-by-trit multiplication
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(self.acc.tritwise_mul(val));
                }

                Opcode::Sat => {
                    // Setun-70 B21 (SAT 000+-+): trit-by-trit threshold addition
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(self.acc.tritwise_sat(val));
                }

                Opcode::Sit => {
                    // Setun-70 B20 (SIT 000+-0): S + T * (1 - |S|) trit-by-trit
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    self.set_acc(self.acc.tritwise_sit(val));
                }

                Opcode::NormT => {
                    // NORM.T / CLZ.T (+27): Normalizes ACC mantissa and writes shifted trit count to IX[reg]
                    let prev_acc = self.acc;
                    let (norm_acc, shift_count) = prev_acc.normalize_leading();
                    let count_word = Word21::new(shift_count as i64);
                    self.set_ix(instr.reg_idx, count_word);
                    self.set_acc(norm_acc);
                }

                Opcode::And => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let res = self.acc.as_i64() & val.as_i64();
                    self.set_acc(Word21::new(res));
                }

                Opcode::Or => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let res = self.acc.as_i64() | val.as_i64();
                    self.set_acc(Word21::new(res));
                }

                Opcode::Xor => {
                    let addr = instr.operand_as_address();
                    let val = read_mem(mem, addr)?;
                    let res = self.acc.as_i64() ^ val.as_i64();
                    self.set_acc(Word21::new(res));
                }

                Opcode::Halt => {
                    self.halted = true;
                    self.cycles += 1;
                    return Ok(false);
                }

                Opcode::Syscall => {
                    let sys_num = self.acc.as_i64();
                    self.cycles += 1;
                    return Err(CpuTrap::Syscall {
                        pc: instr_pc,
                        num: sys_num,
                    });
                }
            }
            Ok(true)
        })();

        match exec_res {
            Ok(active) => {
                if active {
                    self.cycles += 1;
                }
                Ok(active)
            }
            Err(trap) => {
                self.trap = Some(trap);
                Err(trap)
            }
        }
    }

    /// Executes a single Fetch-Decode-Execute cycle (`step()`).
    /// Aborts via panic if any `CpuTrap` occurs (silent failure prohibited).
    #[inline(always)]
    pub fn step(&mut self, mem: &mut Memory) -> bool {
        self.step_checked(mem)
            .unwrap_or_else(|trap| panic!("SETUN-DOOM HARDWARE TRAP: {:?}", trap))
    }

    /// Resumes execution after handling a `CpuTrap::Syscall`.
    #[inline(always)]
    pub fn resume_from_syscall(&mut self, return_val: Word21) {
        self.trap = None;
        self.set_acc(return_val);
    }

    /// Executes up to `count` CPU cycles or until reaching a legitimate `HALT` instruction.
    /// Aborts via panic if any `CpuTrap` occurs.
    pub fn run_ticks(&mut self, mem: &mut Memory, count: usize) -> usize {
        let mut executed = 0;
        while executed < count {
            if !self.step(mem) {
                break;
            }
            executed += 1;
        }
        executed
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FRAMEBUFFER_BASE;
    use crate::word::Tryte6;

    #[test]
    fn test_instruction_encode_decode_roundtrip() {
        let cases = [
            Instruction::new(Opcode::Add, 0, 123_456),
            Instruction::new(Opcode::Sub, 4, -2_000_000),
            Instruction::new(Opcode::LoadIndex, 8, 4_600_000),
            Instruction::new(Opcode::JmpN, 2, 1024),
            Instruction::new(Opcode::JmpZ, 3, 2048),
            Instruction::new(Opcode::JmpP, 7, 4096),
            Instruction::new(Opcode::Call, 1, 512),
            Instruction::new(Opcode::Ret, 0, 0),
        ];

        for &orig in &cases {
            let word = orig.encode();
            let decoded = Instruction::decode(word).expect("Failed to decode instruction");
            assert_eq!(decoded.opcode, orig.opcode);
            assert_eq!(decoded.reg_idx, orig.reg_idx);
            assert_eq!(decoded.operand_as_address(), orig.operand_as_address());
        }
    }

    #[test]
    fn test_trichotomous_branches_without_flags() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Program testing JMPN, JMPZ, and JMPP by directly inspecting ACC sign:
        // Address 100: value -50
        // Address 101: value 0
        // Address 102: value +50
        // Address 200: success result location
        mem.write_word(100, Word21::new(-50)).unwrap();
        mem.write_word(101, Word21::new(0)).unwrap();
        mem.write_word(102, Word21::new(50)).unwrap();

        let prog = [
            // 0: Loads -50 and tests JMPN -> must jump to 3
            Instruction::new(Opcode::Load, 0, 100),
            Instruction::new(Opcode::JmpN, 0, 3),
            Instruction::new(Opcode::Halt, 0, 0), // 2: Fails if branch not taken
            // 3: Neg (-(-50) = +50) and tests JMPP -> must jump to 6
            Instruction::new(Opcode::Neg, 0, 0),
            Instruction::new(Opcode::JmpP, 0, 6),
            Instruction::new(Opcode::Halt, 0, 0), // 5: Fails if branch not taken
            // 6: Subtracts MEM[102] (+50 - 50 = 0) and tests JMPZ -> must jump to 9
            Instruction::new(Opcode::Sub, 0, 102),
            Instruction::new(Opcode::JmpZ, 0, 9),
            Instruction::new(Opcode::Halt, 0, 0), // 8: Fails if branch not taken
            // 9: Success! Writes 777 to address 200 and halts
            Instruction::new(Opcode::LoadImm, 0, 777),
            Instruction::new(Opcode::Store, 0, 200),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 100);

        assert!(cpu.halted);
        assert_eq!(mem.read_word(200).unwrap().as_i64(), 777);
    }

    #[test]
    fn test_stack_call_ret_and_indexed_framebuffer_write() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Subroutine at 50: writes ACC to Framebuffer[IX[1]] and returns
        let sub = [
            Instruction::new(Opcode::StoreIndex, 1, FRAMEBUFFER_BASE as i64),
            Instruction::new(Opcode::Ret, 0, 0),
        ];
        cpu.load_program(&mut mem, 50, &sub);

        let color = Tryte6::from_palette_index(180);
        let main_prog = [
            Instruction::new(Opcode::LoadIxImm, 1, 325), // Pixel offset 325
            Instruction::new(Opcode::LoadImm, 0, color.as_i64()),
            Instruction::new(Opcode::Call, 0, 50),
            Instruction::new(Opcode::Halt, 0, 0),
        ];
        cpu.load_program(&mut mem, 0, &main_prog);
        cpu.run_ticks(&mut mem, 50);

        assert!(cpu.halted);
        assert_eq!(cpu.sp, INITIAL_STACK_POINTER);
        let fb_val = mem
            .read_word((FRAMEBUFFER_BASE + 325) as i64)
            .unwrap()
            .as_i64();
        assert_eq!(fb_val, color.as_i64());
    }

    #[test]
    fn test_rebel6_cmpt_majvt_minvt_and_digit_flips_telemetry() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Tests CMP.T (<=>), MAJV.T (majority vote), and Digit Flips telemetry
        // when numbers alternate between positive and negative (scenario highlighted in Section V.B of REBEL-6 paper)
        mem.write_word(400, Word21::new(100)).unwrap();
        mem.write_word(401, Word21::new(-100)).unwrap();

        let prog = [
            // Alternates ACC among -1, 0, +1, -2, +2 to measure digit flip savings versus two's complement
            Instruction::new(Opcode::LoadImm, 0, -1),
            Instruction::new(Opcode::LoadImm, 0, 1),
            Instruction::new(Opcode::LoadImm, 0, -1),
            Instruction::new(Opcode::LoadImm, 0, 0),
            Instruction::new(Opcode::LoadImm, 0, -1),
            // Tests CMP.T: ACC (-1) <=> MEM[400] (100) -> -1
            Instruction::new(Opcode::CmpT, 0, 400),
            Instruction::new(Opcode::Store, 0, 410),
            // Tests CMP.T: ACC (100) <=> MEM[401] (-100) -> +1
            Instruction::new(Opcode::Load, 0, 400),
            Instruction::new(Opcode::CmpT, 0, 401),
            Instruction::new(Opcode::Store, 0, 411),
            // Tests MAJV.T and MINV.T:
            // ACC = +1, IX[0] = +1, MEM[401_sign] = -1 -> Majority = +1, Minority = -1
            Instruction::new(Opcode::LoadImm, 0, 1),
            Instruction::new(Opcode::LoadIxImm, 0, 1),
            Instruction::new(Opcode::MajvT, 0, 410), // MEM[410] is -1 -> Maj(+1, +1, -1) = +1
            Instruction::new(Opcode::Store, 0, 412),
            Instruction::new(Opcode::MinvT, 0, 410), // Minv(+1, +1, -1) = -1
            Instruction::new(Opcode::Store, 0, 413),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 100);

        assert!(cpu.halted);
        assert_eq!(mem.read_word(410).unwrap().as_i64(), -1);
        assert_eq!(mem.read_word(411).unwrap().as_i64(), 1);
        assert_eq!(mem.read_word(412).unwrap().as_i64(), 1);
        assert_eq!(mem.read_word(413).unwrap().as_i64(), -1);

        // Verifies that ternary Trit Flips are substantially lower than binary 32-bit Bit Flips
        assert!(cpu.telemetry.ternary_trit_flips < cpu.telemetry.binary32_bit_flips);
        assert!(cpu.telemetry.digit_flip_reduction_pct() > 50.0);
    }

    #[test]
    fn test_bceg_three_way_branch_and_compound_jumps() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // Tests BCEG dispatching to 3 distinct routes in a single instruction:
        // Compares ACC with IX[0] (value 10):
        // - Case 1: ACC = 5  (< 10) -> fallthrough (PC + 1) -> writes 111 to MEM[300]
        // - Case 2: ACC = 10 (== 10) -> jumps offset_eq (+4) -> writes 222 to MEM[301]
        // - Case 3: ACC = 20 (> 10) -> jumps offset_gt (+7) -> writes 333 to MEM[302]
        let prog = [
            // 0: IX[0] = 10 (reference value for BCEG)
            Instruction::new(Opcode::LoadIxImm, 0, 10),
            // 1: Case 1: ACC = 5 (< IX[0])
            Instruction::new(Opcode::LoadImm, 0, 5),
            // 2: BCEG IX0, eq=+4 (goes to 6), gt=+7 (goes to 9) -> since 5 < 10, falls through to 3
            Instruction::new_bceg(0, 4, 7),
            // 3: Route Less (< 10): writes 111 to MEM[300] and moves to Case 2 (PC = 5)
            Instruction::new(Opcode::LoadImm, 0, 111),
            Instruction::new(Opcode::Store, 0, 300),
            // 5: Case 2: ACC = 10 (== IX[0]) -> BCEG at 6 jumps to eq (+3 -> PC = 9)
            Instruction::new(Opcode::LoadImm, 0, 10),
            // 6: BCEG IX0, eq=+3 (goes to 9), gt=+6 (goes to 12)
            Instruction::new_bceg(0, 3, 6),
            // 7..8: trap if branch not taken
            Instruction::new(Opcode::Halt, 0, 0),
            Instruction::new(Opcode::Halt, 0, 0),
            // 9: Route Equal (== 10): writes 222 to MEM[301] and tests Case 3
            Instruction::new(Opcode::LoadImm, 0, 222),
            Instruction::new(Opcode::Store, 0, 301),
            // 11: Case 3: ACC = 20 (> IX[0])
            Instruction::new(Opcode::LoadImm, 0, 20),
            // 12: BCEG IX0, eq=+2 (goes to 14), gt=+3 (goes to 15)
            Instruction::new_bceg(0, 2, 3),
            // 13..14: trap if branch to gt not taken
            Instruction::new(Opcode::Halt, 0, 0),
            Instruction::new(Opcode::Halt, 0, 0),
            // 15: Route Greater (> 10): writes 333 to MEM[302] and halts
            Instruction::new(Opcode::LoadImm, 0, 333),
            Instruction::new(Opcode::Store, 0, 302),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 100);

        assert!(cpu.halted);
        assert_eq!(mem.read_word(300).unwrap().as_i64(), 111);
        assert_eq!(mem.read_word(301).unwrap().as_i64(), 222);
        assert_eq!(mem.read_word(302).unwrap().as_i64(), 333);
    }

    #[test]
    fn test_cpu_traps_prohibit_silent_failures() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // 1. Invalid opcode (99 in top 5 trits) must raise CpuTrap::InvalidInstruction
        let invalid_word = Word21::new(99 * crate::word::POW3[16]);
        mem.write_word(0, invalid_word).unwrap();
        let res1 = cpu.step_checked(&mut mem);
        assert_eq!(
            res1,
            Err(CpuTrap::InvalidInstruction {
                pc: 0,
                raw_word: invalid_word
            })
        );

        // 2. Division by zero in Opcode::DivInt must raise CpuTrap::DivisionByZero
        cpu.reset(10);
        mem.write_word(500, Word21::ZERO).unwrap();
        let div_zero_prog = [
            Instruction::new(Opcode::LoadImm, 0, 42),
            Instruction::new(Opcode::DivInt, 0, 500),
            Instruction::new(Opcode::Halt, 0, 0),
        ];
        cpu.load_program(&mut mem, 10, &div_zero_prog);
        assert_eq!(cpu.step_checked(&mut mem), Ok(true));
        assert_eq!(
            cpu.step_checked(&mut mem),
            Err(CpuTrap::DivisionByZero {
                pc: 11,
                opcode: Opcode::DivInt
            })
        );

        // 3. Access outside 14-trit addressing space (e.g. PC = 9_999_999) must raise CpuTrap::MemoryFault
        cpu.reset(9_999_999);
        assert_eq!(
            cpu.step_checked(&mut mem),
            Err(CpuTrap::MemoryFault {
                pc: 9_999_999,
                error: crate::memory::MemoryError::OutOfBounds(9_999_999)
            })
        );
    }

    #[test]
    fn test_setun58_setun70_authentic_instructions_and_alvarez_converter() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // 1. Verifies Ramil Alvarez (1974) 2->3 recurrent base converter across logic gates
        for sample in [0i32, 1, -1, 42, -42, 59_049, -59_049, i32::MAX, i32::MIN] {
            let (trits, carries) = Word21::alvarez_from_i32_gates(sample);
            let w = Word21::from_trits(&trits);
            assert_eq!(w.as_i64(), sample as i64);
            if sample.unsigned_abs() > 1 {
                assert!(carries > 0);
            }
        }

        // 2. Verifies MUL_ZERO (++0), MAC_POS (+++ / LBT), and MAC_NEG (++-):
        // 2D BSP cross product: dx * ldy - dy * ldx
        // Let dx = 4.0, ldy = 3.0, dy = 2.0, ldx = 5.0 -> 4*3 - 2*5 = 12 - 10 = 2.0
        mem.write_word(600, TernaryFixed::from_int(4).raw_word())
            .unwrap(); // dx = 4.0
        mem.write_word(601, TernaryFixed::from_int(3).raw_word())
            .unwrap(); // ldy = 3.0
        mem.write_word(602, TernaryFixed::from_int(2).raw_word())
            .unwrap(); // dy = 2.0
        mem.write_word(603, TernaryFixed::from_int(5).raw_word())
            .unwrap(); // ldx = 5.0

        // 3. Verifies SMT (B19), SAT (B21), and SIT (B20) from Setun-70:
        // Let A = 0t+-0 and B = 0t+--
        // SMT(+-0, +--) = (+*+, -*-, 0*-) = ++0 (+1*9 + 1*3 + 0 = 12)
        // SAT(+-0, +--) = (thresh(+,+), thresh(-,-), thresh(0,-)) = +-- (+1*9 - 1*3 - 1 = 5)
        // SIT(+-0, +--) = (+, -, -) since only the Zero trit in A adopts the '-' trit of B = +-- (5)
        let word_a = Word21::from_balanced_str("+-0").unwrap();
        let word_b = Word21::from_balanced_str("+--").unwrap();
        mem.write_word(610, word_a).unwrap();
        mem.write_word(611, word_b).unwrap();

        let prog = [
            // ACC = dx (4.0); MUL 601 (++0 Setun-58) -> R = 4.0, ACC = 3.0 * 4.0 = 12.0
            Instruction::new(Opcode::Load, 0, 600),
            Instruction::new(Opcode::Mul, 0, 601),
            // R = dy (2.0); MAC_NEG 603 -> ACC = 12.0 - 5.0 * 2.0 = 2.0
            Instruction::new(Opcode::LoadR, 0, 602),
            Instruction::new(Opcode::MacNeg, 0, 603),
            Instruction::new(Opcode::Store, 0, 604),
            // MAC_POS 603 -> ACC = 2.0 + 5.0 * 2.0 = 12.0
            Instruction::new(Opcode::MacPos, 0, 603),
            Instruction::new(Opcode::Store, 0, 605),
            // SMT: A (*) B -> ++0 (12)
            Instruction::new(Opcode::Load, 0, 610),
            Instruction::new(Opcode::Smt, 0, 611),
            Instruction::new(Opcode::Store, 0, 612),
            // SAT: ThresholdAdd(A, B) -> +-- (5)
            Instruction::new(Opcode::Load, 0, 610),
            Instruction::new(Opcode::Sat, 0, 611),
            Instruction::new(Opcode::Store, 0, 613),
            // SIT: A + B * (1 - |A|) -> +-- (5)
            Instruction::new(Opcode::Load, 0, 610),
            Instruction::new(Opcode::Sit, 0, 611),
            Instruction::new(Opcode::Store, 0, 614),
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 50);

        assert!(cpu.halted);
        assert_eq!(
            mem.read_word(604).unwrap(),
            TernaryFixed::from_int(2).raw_word()
        );
        assert_eq!(
            mem.read_word(605).unwrap(),
            TernaryFixed::from_int(12).raw_word()
        );
        assert_eq!(mem.read_word(612).unwrap().as_i64(), 12);
        assert_eq!(mem.read_word(613).unwrap().as_i64(), 5);
        assert_eq!(mem.read_word(614).unwrap().as_i64(), 5);
    }

    #[test]
    fn test_norm_t_clz_t_execution() {
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();

        // 1. Zero word case: lz = 21, ACC = 0
        // 2. Active trit 20 case (+1): lz = 0, ACC unchanged
        // 3. 5 leading zero trits case: lz = 5, ACC shifted 5 positions
        // 4. Negative symmetric case (-1): lz = 5, ACC shifted 5 positions
        let mut trits_5z = [Trit::Zero; 21];
        // 5 leading zero trits (indices 20..16 = 0)
        // index 15 = +1
        trits_5z[15] = Trit::Positive;
        trits_5z[13] = Trit::Negative;
        trits_5z[11] = Trit::Positive;
        let word_5_zeros = Word21::from_trits(&trits_5z);
        let word_5_zeros_neg = word_5_zeros.gate_invert_trits();
        mem.write_word(700, word_5_zeros).unwrap();
        mem.write_word(701, word_5_zeros_neg).unwrap();

        let prog = [
            // Test 1: NORM.T on zero
            Instruction::new(Opcode::LoadImm, 0, 0),
            Instruction::new(Opcode::NormT, 1, 0),   // IX1 = lz
            Instruction::new(Opcode::Store, 0, 710), // writes ACC
            Instruction::new(Opcode::MovIxToAcc, 1, 0),
            Instruction::new(Opcode::Store, 0, 711), // writes IX1
            // Test 2: NORM.T with 5 leading zeros
            Instruction::new(Opcode::Load, 0, 700),
            Instruction::new(Opcode::NormT, 2, 0),   // IX2 = 5
            Instruction::new(Opcode::Store, 0, 712), // writes normalized ACC
            Instruction::new(Opcode::MovIxToAcc, 2, 0),
            Instruction::new(Opcode::Store, 0, 713), // writes IX2 (5)
            // Test 3: NORM.T with 5 leading zeros negative (exact symmetry)
            Instruction::new(Opcode::Load, 0, 701),
            Instruction::new(Opcode::NormT, 3, 0),   // IX3 = 5
            Instruction::new(Opcode::Store, 0, 714), // writes normalized ACC
            Instruction::new(Opcode::MovIxToAcc, 3, 0),
            Instruction::new(Opcode::Store, 0, 715), // writes IX3 (5)
            Instruction::new(Opcode::Halt, 0, 0),
        ];

        cpu.load_program(&mut mem, 0, &prog);
        cpu.run_ticks(&mut mem, 50);

        assert!(cpu.halted);
        assert_eq!(mem.read_word(710).unwrap(), Word21::ZERO);
        assert_eq!(mem.read_word(711).unwrap().as_i64(), 21);

        let norm_expected = word_5_zeros.shift_trits(5);
        assert_eq!(mem.read_word(712).unwrap(), norm_expected);
        assert_eq!(mem.read_word(713).unwrap().as_i64(), 5);

        let norm_neg_expected = word_5_zeros_neg.shift_trits(5);
        assert_eq!(mem.read_word(714).unwrap(), norm_neg_expected);
        assert_eq!(mem.read_word(715).unwrap().as_i64(), 5);
        assert_eq!(norm_expected.as_i64(), -norm_neg_expected.as_i64());
    }

    #[test]
    fn test_energy_reduction_requires_binary_reference() {
        // Without a binary reference counter, no percentage may be reported.
        assert_eq!(
            PowerTelemetry::default().weighted_energy_reduction_opt(),
            None
        );

        // Ternary energy = 40 half-swings * 0.25 + 10 full-swings = 20 against 200 binary toggles.
        let measured = PowerTelemetry {
            ternary_half_swings: 40,
            ternary_full_swings: 10,
            binary32_bit_flips: 200,
            ..PowerTelemetry::default()
        };
        assert_eq!(measured.weighted_energy_reduction_opt(), Some(90.0));
    }
}
