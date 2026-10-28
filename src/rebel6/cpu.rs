//! REBEL-6 execution core.
//!
//! Harvard machine: `instrs` is the instruction array (32 trits per slot, one PC step per
//! instruction), `data` is byte/tryte addressed memory. Registers are 24-trit balanced words;
//! `X0` is hardwired to zero. Ternary arithmetic wraps modulo `3^24`; the Base Binary layer
//! reinterprets the same registers as `i32` and wraps modulo `2^32`.

use crate::rebel6::encoding::{decode, Op};
use crate::rebel6::mem::{DataMem, MemError};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::rebel6::word::Word24;

/// Number of architectural registers (`3^6`).
pub const REGISTERS: usize = 729;

/// Architectural trap.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Trap {
    /// Opcode or func outside the implemented set.
    InvalidInstruction {
        /// Program counter of the faulting slot.
        pc: i64,
        /// Raw 32-trit word.
        word: u64,
    },
    /// Register index outside `0..729`.
    InvalidRegister {
        /// Encoded register index.
        index: i64,
    },
    /// Data access outside the populated window.
    MemoryFault {
        /// Faulting address.
        addr: i64,
    },
    /// `EBREAK.T`.
    Breakpoint {
        /// Program counter of the breakpoint.
        pc: i64,
    },
    /// `TRET.T` without a trap frame.
    TrapReturn,
    /// `WFI.T` without an interrupt controller.
    WaitForInterrupt,
    /// The step budget ran out.
    StepLimit,
}

/// Result of a single step.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Instruction retired.
    Continue,
    /// `ECALL.T`, carrying the value of `a7` (`X17`).
    Ecall(i64),
    /// The program counter left the instruction array.
    Halt(i64),
}

/// Result of a run loop.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RunResult {
    /// The program counter left the instruction array.
    Halt(i64),
    /// The guest issued `ECALL.T`.
    Ecall(i64),
}

/// REBEL-6 machine state.
pub struct Cpu {
    /// 729 registers of 24 trits.
    pub regs: [Word24; REGISTERS],
    /// Program counter in instruction slots.
    pub pc: i64,
    /// Instruction memory.
    pub instrs: Vec<u64>,
    /// Data memory.
    pub data: DataMem,
    /// Retired instructions.
    pub cycles: u64,
    /// Register trits that changed value.
    pub trit_flips: u64,
    /// Weighted switching energy in quarter units.
    pub energy: u64,
    /// Set when the program counter leaves instruction memory.
    pub halted: bool,
    /// Exit code reported on halt.
    pub exit_code: i64,
    /// Prints every register write when enabled.
    pub trace: bool,
}

impl Cpu {
    /// Creates a machine with the given instruction image and data size.
    #[must_use]
    pub fn new(instrs: Vec<u64>, data_size: usize) -> Self {
        Self {
            regs: [Word24::ZERO; REGISTERS],
            pc: 0,
            instrs,
            data: DataMem::new(data_size),
            cycles: 0,
            trit_flips: 0,
            energy: 0,
            halted: false,
            exit_code: 0,
            trace: false,
        }
    }

    /// Sets the program counter.
    pub fn set_pc(&mut self, pc: i64) {
        self.pc = pc;
    }

    /// Reads a register. `X0` always reads zero.
    ///
    /// # Errors
    ///
    /// Returns [`Trap::InvalidRegister`] for an index outside `0..729`.
    pub fn read_reg(&self, index: i64) -> Result<Word24, Trap> {
        if index == 0 {
            Ok(Word24::ZERO)
        } else if (0..REGISTERS as i64).contains(&index) {
            Ok(self.regs[index as usize])
        } else {
            Err(Trap::InvalidRegister { index })
        }
    }

    /// Writes a register and accounts for switching. `X0` discards writes.
    ///
    /// # Errors
    ///
    /// Returns [`Trap::InvalidRegister`] for an index outside `0..729`.
    pub fn write_reg(&mut self, index: i64, value: Word24) -> Result<(), Trap> {
        if index == 0 {
            return Ok(());
        }
        if !(0..REGISTERS as i64).contains(&index) {
            return Err(Trap::InvalidRegister { index });
        }
        let slot = &mut self.regs[index as usize];
        let old = *slot;
        let flips = old.trit_flips(value);
        if self.trace {
            #[cfg(feature = "std")]
            println!("store x{index} {old} -> {value} flips {flips}");
        }
        self.trit_flips += u64::from(flips);
        self.energy += u64::from(old.weighted_energy(value));
        *slot = value;
        Ok(())
    }

    fn mem_trap(err: MemError) -> Trap {
        match err {
            MemError::OutOfBounds { addr, .. } => Trap::MemoryFault { addr },
        }
    }

    fn data_addr(rs1: Word24, imm: i64) -> i64 {
        rs1.raw() + imm
    }

    /// Executes one instruction.
    ///
    /// # Errors
    ///
    /// Returns an architectural [`Trap`] on an invalid instruction, register, memory access,
    /// breakpoint, trap return or wait.
    pub fn step(&mut self) -> Result<Outcome, Trap> {
        if self.pc < 0 || self.pc as usize >= self.instrs.len() {
            self.halted = true;
            return Ok(Outcome::Halt(self.exit_code));
        }
        let word = self.instrs[self.pc as usize];
        let instr = decode(word).ok_or(Trap::InvalidInstruction { pc: self.pc, word })?;
        self.cycles += 1;
        let pc = self.pc;
        let mut next = pc + 1;
        let r = |cpu: &Self, index: i64| cpu.read_reg(index);

        match instr.op {
            // ---------------------------------------------------------------- Base Ternary
            Op::AddT => {
                let v = r(self, instr.rs1)?.add(r(self, instr.rs2)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::SubT => {
                let v = r(self, instr.rs1)?.sub(r(self, instr.rs2)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::ShT => {
                let v = r(self, instr.rs1)?.shift(r(self, instr.rs2)?.low(4) as i32, instr.fill);
                self.write_reg(instr.rd1, v)?;
            }
            Op::RotrT => {
                let v = r(self, instr.rs1)?.rotate(r(self, instr.rs2)?.low(4) as i32);
                self.write_reg(instr.rd1, v)?;
            }
            Op::SltT => {
                let v = if r(self, instr.rs1)? < r(self, instr.rs2)? {
                    Word24::ONE
                } else {
                    Word24::ZERO
                };
                self.write_reg(instr.rd1, v)?;
            }
            Op::OrT => {
                let v = r(self, instr.rs1)?.or_tritwise(r(self, instr.rs2)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::XorT => {
                let v = r(self, instr.rs1)?.xor_tritwise(r(self, instr.rs2)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::AndT => {
                let v = r(self, instr.rs1)?.and_tritwise(r(self, instr.rs2)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::CmpT => {
                let v = Word24::from_raw(i64::from(r(self, instr.rs1)?.cmp3(r(self, instr.rs2)?)));
                self.write_reg(instr.rd1, v)?;
            }
            Op::StiT => {
                let v = r(self, instr.rs1)?.neg();
                self.write_reg(instr.rd1, v)?;
            }
            Op::Mv2T => {
                let a = r(self, instr.rs1)?;
                let b = r(self, instr.rs2)?;
                self.write_reg(instr.rd1, a)?;
                self.write_reg(instr.rd2, b)?;
            }
            Op::MinT => {
                let v = r(self, instr.rs1)?.min(r(self, instr.rs2)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::MaxT => {
                let v = r(self, instr.rs1)?.max(r(self, instr.rs2)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::AddiT => {
                let v = r(self, instr.rs1)?.add(Word24::new(instr.imm));
                self.write_reg(instr.rd1, v)?;
            }
            Op::ShiT => {
                let v = r(self, instr.rs1)?.shift(instr.imm as i32, instr.fill);
                self.write_reg(instr.rd1, v)?;
            }
            Op::RotT => {
                let v = r(self, instr.rs1)?.rotate(instr.imm as i32);
                self.write_reg(instr.rd1, v)?;
            }
            Op::SltiT => {
                let v = if r(self, instr.rs1)?.raw() < instr.imm {
                    Word24::ONE
                } else {
                    Word24::ZERO
                };
                self.write_reg(instr.rd1, v)?;
            }
            Op::OriT => {
                let v = r(self, instr.rs1)?.or_tritwise(Word24::new(instr.imm));
                self.write_reg(instr.rd1, v)?;
            }
            Op::XoriT => {
                let v = r(self, instr.rs1)?.xor_tritwise(Word24::new(instr.imm));
                self.write_reg(instr.rd1, v)?;
            }
            Op::AndiT => {
                let v = r(self, instr.rs1)?.and_tritwise(Word24::new(instr.imm));
                self.write_reg(instr.rd1, v)?;
            }
            Op::LwT => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v = Word24::from_i32(self.data.load_i32(addr).map_err(Self::mem_trap)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::LhT => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v =
                    Word24::from_i32(i32::from(self.data.load_i16(addr).map_err(Self::mem_trap)?));
                self.write_reg(instr.rd1, v)?;
            }
            Op::LtT => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v =
                    Word24::from_i32(i32::from(self.data.load_i8(addr).map_err(Self::mem_trap)?));
                self.write_reg(instr.rd1, v)?;
            }
            Op::JalrT => {
                let target = r(self, instr.rs1)?.raw() + instr.imm;
                self.write_reg(instr.rd1, Word24::new(pc + 1))?;
                next = target;
            }
            Op::BcgsT => {
                let a = r(self, instr.rs1)?;
                let b = r(self, instr.rs2)?;
                match a.cmp3(b) {
                    1 => next = pc + instr.imm,
                    -1 => next = pc + instr.imm2,
                    _ => {}
                }
            }
            Op::BcegT => {
                let a = r(self, instr.rs1)?;
                let b = r(self, instr.rs2)?;
                match a.cmp3(b) {
                    0 => next = pc + instr.imm,
                    1 => next = pc + instr.imm2,
                    _ => {}
                }
            }
            Op::BeqT | Op::BneT | Op::BltT | Op::BgeT => {
                let a = r(self, instr.rs1)?;
                let b = r(self, instr.rs2)?;
                let taken = match instr.op {
                    Op::BeqT => a == b,
                    Op::BneT => a != b,
                    Op::BltT => a < b,
                    Op::BgeT => a >= b,
                    _ => unreachable!(),
                };
                if taken {
                    next = pc + instr.imm;
                }
            }
            Op::SwT => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                self.data
                    .store_u32(addr, r(self, instr.rs2)?.to_i32() as u32)
                    .map_err(Self::mem_trap)?;
            }
            Op::ShTStore => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                self.data
                    .store_u16(addr, r(self, instr.rs2)?.to_i32() as u16)
                    .map_err(Self::mem_trap)?;
            }
            Op::StT => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                self.data
                    .store_u8(addr, r(self, instr.rs2)?.to_i32() as u8)
                    .map_err(Self::mem_trap)?;
            }
            Op::MajvT => {
                let v = r(self, instr.rs1)?.majority(r(self, instr.rs2)?, r(self, instr.rs3)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::MinvT => {
                let v = r(self, instr.rs1)?.minority(r(self, instr.rs2)?, r(self, instr.rs3)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::Li2T => {
                self.write_reg(instr.rd1, Word24::new(instr.imm))?;
                self.write_reg(instr.rd2, Word24::new(instr.imm2))?;
            }
            Op::FenceT => {}
            Op::WfiT => return Err(Trap::WaitForInterrupt),
            Op::TretT => return Err(Trap::TrapReturn),
            Op::EbreakT => return Err(Trap::Breakpoint { pc }),
            Op::EcallT => {
                self.pc = next;
                return Ok(Outcome::Ecall(r(self, 17)?.raw()));
            }
            Op::LwaT => {
                let v = Word24::from_i32(self.data.load_i32(instr.imm).map_err(Self::mem_trap)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::LiT => {
                self.write_reg(instr.rd1, Word24::new(instr.imm))?;
            }
            Op::SwaT => {
                self.data
                    .store_u32(instr.imm, r(self, instr.rs1)?.to_i32() as u32)
                    .map_err(Self::mem_trap)?;
            }
            Op::JalT => {
                self.write_reg(instr.rd1, Word24::new(pc + 1))?;
                next = pc + instr.imm;
            }
            Op::AipcT => {
                self.write_reg(instr.rd1, Word24::new(pc + instr.imm))?;
            }

            // ---------------------------------------------------------------- Base Binary
            Op::Add => {
                let v = r(self, instr.rs1)?
                    .to_i32()
                    .wrapping_add(r(self, instr.rs2)?.to_i32());
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Sub => {
                let v = r(self, instr.rs1)?
                    .to_i32()
                    .wrapping_sub(r(self, instr.rs2)?.to_i32());
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Sll => {
                let sh = (r(self, instr.rs2)?.to_i32() as u32) & 31;
                let v = r(self, instr.rs1)?.to_i32().wrapping_shl(sh);
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Srl => {
                let sh = (r(self, instr.rs2)?.to_i32() as u32) & 31;
                let v = ((r(self, instr.rs1)?.to_i32() as u32).wrapping_shr(sh)) as i32;
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Sra => {
                let sh = (r(self, instr.rs2)?.to_i32() as u32) & 31;
                let v = r(self, instr.rs1)?.to_i32().wrapping_shr(sh);
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Sltu => {
                let v = i32::from(
                    (r(self, instr.rs1)?.to_i32() as u32) < (r(self, instr.rs2)?.to_i32() as u32),
                );
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Or => {
                let v = r(self, instr.rs1)?.to_i32() | r(self, instr.rs2)?.to_i32();
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Xor => {
                let v = r(self, instr.rs1)?.to_i32() ^ r(self, instr.rs2)?.to_i32();
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::And => {
                let v = r(self, instr.rs1)?.to_i32() & r(self, instr.rs2)?.to_i32();
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Addi => {
                let v = r(self, instr.rs1)?.to_i32().wrapping_add(instr.imm as i32);
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Slli => {
                let v = r(self, instr.rs1)?
                    .to_i32()
                    .wrapping_shl((instr.imm as u32) & 31);
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Srli => {
                let v = ((r(self, instr.rs1)?.to_i32() as u32)
                    .wrapping_shr((instr.imm as u32) & 31)) as i32;
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Srai => {
                let v = r(self, instr.rs1)?
                    .to_i32()
                    .wrapping_shr((instr.imm as u32) & 31);
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Sltiu => {
                let v = i32::from((r(self, instr.rs1)?.to_i32() as u32) < (instr.imm as u32));
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Ori => {
                let v = r(self, instr.rs1)?.to_i32() | instr.imm as i32;
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Xori => {
                let v = r(self, instr.rs1)?.to_i32() ^ instr.imm as i32;
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Andi => {
                let v = r(self, instr.rs1)?.to_i32() & instr.imm as i32;
                self.write_reg(instr.rd1, Word24::from_i32(v))?;
            }
            Op::Lw => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v = Word24::from_i32(self.data.load_i32(addr).map_err(Self::mem_trap)?);
                self.write_reg(instr.rd1, v)?;
            }
            Op::Lh => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v =
                    Word24::from_i32(i32::from(self.data.load_i16(addr).map_err(Self::mem_trap)?));
                self.write_reg(instr.rd1, v)?;
            }
            Op::Lb => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v =
                    Word24::from_i32(i32::from(self.data.load_i8(addr).map_err(Self::mem_trap)?));
                self.write_reg(instr.rd1, v)?;
            }
            Op::Lhu => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v =
                    Word24::from_i32(i32::from(self.data.load_u16(addr).map_err(Self::mem_trap)?));
                self.write_reg(instr.rd1, v)?;
            }
            Op::Lbu => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                let v =
                    Word24::from_i32(i32::from(self.data.load_u8(addr).map_err(Self::mem_trap)?));
                self.write_reg(instr.rd1, v)?;
            }
            Op::Bltu => {
                let a = r(self, instr.rs1)?.to_i32() as u32;
                let b = r(self, instr.rs2)?.to_i32() as u32;
                if a < b {
                    next = pc + instr.imm;
                }
            }
            Op::Bgeu => {
                let a = r(self, instr.rs1)?.to_i32() as u32;
                let b = r(self, instr.rs2)?.to_i32() as u32;
                if a >= b {
                    next = pc + instr.imm;
                }
            }
            Op::Sw => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                self.data
                    .store_u32(addr, r(self, instr.rs2)?.to_i32() as u32)
                    .map_err(Self::mem_trap)?;
            }
            Op::Sh => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                self.data
                    .store_u16(addr, r(self, instr.rs2)?.to_i32() as u16)
                    .map_err(Self::mem_trap)?;
            }
            Op::Sb => {
                let addr = Self::data_addr(r(self, instr.rs1)?, instr.imm);
                self.data
                    .store_u8(addr, r(self, instr.rs2)?.to_i32() as u8)
                    .map_err(Self::mem_trap)?;
            }
        }
        self.pc = next;
        Ok(Outcome::Continue)
    }

    /// Runs until an `ECALL.T`, a halt, or `max_steps`.
    ///
    /// # Errors
    ///
    /// Returns the first architectural trap, or [`Trap::StepLimit`] when the budget is exhausted.
    pub fn run(&mut self, max_steps: u64) -> Result<RunResult, Trap> {
        for _ in 0..max_steps {
            match self.step()? {
                Outcome::Continue => {}
                Outcome::Ecall(num) => return Ok(RunResult::Ecall(num)),
                Outcome::Halt(code) => return Ok(RunResult::Halt(code)),
            }
        }
        Err(Trap::StepLimit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebel6::encoding::{encode, Instr, Op};

    fn image(instrs: &[Instr]) -> Vec<u64> {
        instrs.iter().map(encode).collect()
    }

    #[test]
    fn ternary_arithmetic_and_ecall() {
        let instrs = image(&[
            Instr::g(Op::LiT, 1, 5),
            Instr::i(Op::AddiT, 2, 1, 7),
            Instr::r(Op::AddT, 3, 1, 2),
            Instr::new(Op::EcallT),
        ]);
        let mut cpu = Cpu::new(instrs, 16);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[2].raw(), 12);
        assert_eq!(cpu.regs[3].raw(), 17);
    }

    #[test]
    fn binary_arithmetic_wraps_modulo_2_pow_32() {
        let instrs = image(&[
            Instr::g(Op::LiT, 1, 0x7FFF_FFFF),
            Instr::i(Op::Addi, 2, 1, 1),
            Instr::new(Op::EcallT),
        ]);
        let mut cpu = Cpu::new(instrs, 16);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[2].to_i32(), i32::MIN);
    }

    #[test]
    fn ternary_addition_wraps_modulo_3_pow_24() {
        let instrs = image(&[
            Instr::g(Op::LiT, 1, crate::rebel6::word::HALF),
            Instr::i(Op::AddiT, 2, 1, 1),
            Instr::new(Op::EcallT),
        ]);
        let mut cpu = Cpu::new(instrs, 16);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[2].raw(), -crate::rebel6::word::HALF);
    }

    #[test]
    fn load_store_round_trip() {
        let instrs = image(&[
            Instr::g(Op::LiT, 1, 0x1234_5678),
            Instr::g(Op::LiT, 2, 64),
            Instr::b(Op::Sw, 2, 1, 0),
            Instr::i(Op::Lw, 3, 2, 0),
            Instr::new(Op::EcallT),
        ]);
        let mut cpu = Cpu::new(instrs, 128);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[3].to_i32(), 0x1234_5678);
    }

    #[test]
    fn branches_and_jumps() {
        let instrs = image(&[
            Instr::g(Op::LiT, 1, 1),
            Instr::g(Op::LiT, 2, 2),
            Instr::b(Op::BltT, 1, 2, 2),
            Instr::g(Op::LiT, 3, 99),
            Instr::g(Op::LiT, 4, 7),
            Instr::new(Op::EcallT),
        ]);
        let mut cpu = Cpu::new(instrs, 16);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[3].raw(), 0);
        assert_eq!(cpu.regs[4].raw(), 7);
    }

    #[test]
    fn jal_and_jalr_return_addresses() {
        let instrs = image(&[
            Instr::g(Op::JalT, 1, 2),
            Instr::g(Op::LiT, 2, 99),
            Instr::i(Op::JalrT, 0, 1, 2),
            Instr::new(Op::EcallT),
        ]);
        let mut cpu = Cpu::new(instrs, 16);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[1].raw(), 1);
        assert_eq!(cpu.regs[2].raw(), 0);
    }

    #[test]
    fn x0_is_hardwired() {
        let instrs = image(&[Instr::g(Op::LiT, 0, 42), Instr::new(Op::EcallT)]);
        let mut cpu = Cpu::new(instrs, 16);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.regs[0].raw(), 0);
    }

    #[test]
    fn counts_trit_flips_on_register_writes() {
        let instrs = image(&[
            Instr::g(Op::LiT, 1, 0),
            Instr::g(Op::LiT, 1, 1),
            Instr::new(Op::EcallT),
        ]);
        let mut cpu = Cpu::new(instrs, 16);
        assert_eq!(cpu.run(16), Ok(RunResult::Ecall(0)));
        assert_eq!(cpu.trit_flips, 1);
        assert_eq!(cpu.energy, 1);
    }

    #[test]
    fn invalid_instruction_traps() {
        use crate::rebel6::encoding::put_field;
        let mut word = 0u64;
        put_field(&mut word, 0, 4, 0);
        put_field(&mut word, 4, 4, 4);
        let mut cpu = Cpu::new(vec![word], 16);
        assert!(matches!(
            cpu.step(),
            Err(Trap::InvalidInstruction { pc: 0, .. })
        ));
    }
}
