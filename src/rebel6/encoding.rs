//! 32-trit REBEL-6 instruction encoding.
//!
//! The instruction set itself lives in [`crate::rebel6::isa`]; this module turns a table row into
//! a packed 32-trit word and back. Fields are laid out most-significant-trit first as specified in
//! the REBEL-6 reference, and instructions are stored at two bits per trit in a `u64`, lane 0
//! being the least significant trit.

use crate::rebel6::isa::{op_for_encoding, op_for_long_encoding};

pub use crate::rebel6::isa::spec;
use crate::rebel6::word::POW3;

pub use crate::rebel6::isa::{Format, Op};

/// A decoded instruction with all operand fields.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Instr {
    /// Architectural operation.
    pub op: Op,
    /// First destination register.
    pub rd1: i64,
    /// Second destination register (`MV2.T`) or shift fill.
    pub rd2: i64,
    /// First source register.
    pub rs1: i64,
    /// Second source register.
    pub rs2: i64,
    /// Third source register (`D` format).
    pub rs3: i64,
    /// Immediate, displacement or shift amount.
    pub imm: i64,
    /// Second immediate (`LI2.T`) or second displacement (`BCGS.T`/`BCEG.T`).
    pub imm2: i64,
    /// Shift fill trit for `ShT`/`ShiT`.
    pub fill: i8,
}

impl Instr {
    /// Creates a zeroed instruction for `op`.
    #[must_use]
    pub const fn new(op: Op) -> Self {
        Self {
            op,
            rd1: 0,
            rd2: 0,
            rs1: 0,
            rs2: 0,
            rs3: 0,
            imm: 0,
            imm2: 0,
            fill: 0,
        }
    }

    /// R-format instruction.
    #[must_use]
    pub const fn r(op: Op, rd1: i64, rs1: i64, rs2: i64) -> Self {
        let mut i = Self::new(op);
        i.rd1 = rd1;
        i.rs1 = rs1;
        i.rs2 = rs2;
        i
    }

    /// I-format instruction.
    #[must_use]
    pub const fn i(op: Op, rd1: i64, rs1: i64, imm: i64) -> Self {
        let mut i = Self::new(op);
        i.rd1 = rd1;
        i.rs1 = rs1;
        i.imm = imm;
        i
    }

    /// B-format instruction.
    #[must_use]
    pub const fn b(op: Op, rs1: i64, rs2: i64, imm: i64) -> Self {
        let mut i = Self::new(op);
        i.rs1 = rs1;
        i.rs2 = rs2;
        i.imm = imm;
        i
    }

    /// G-format instruction.
    #[must_use]
    pub const fn g(op: Op, rd1: i64, imm: i64) -> Self {
        let mut i = Self::new(op);
        i.rd1 = rd1;
        i.imm = imm;
        i
    }

    /// Y-format instruction.
    #[must_use]
    pub const fn y(op: Op, rs1: i64, imm: i64) -> Self {
        let mut i = Self::new(op);
        i.rs1 = rs1;
        i.imm = imm;
        i
    }

    /// D-format instruction.
    #[must_use]
    pub const fn d(op: Op, rd1: i64, rs1: i64, rs2: i64, rs3: i64) -> Self {
        let mut i = Self::new(op);
        i.rd1 = rd1;
        i.rs1 = rs1;
        i.rs2 = rs2;
        i.rs3 = rs3;
        i
    }

    /// R-format shift with an explicit fill trit.
    #[must_use]
    pub const fn sh_r(op: Op, rd1: i64, rs1: i64, rs2: i64, fill: i8) -> Self {
        let mut i = Self::r(op, rd1, rs1, rs2);
        i.fill = fill;
        i
    }

    /// I-format shift with an explicit fill trit.
    #[must_use]
    pub const fn sh_i(op: Op, rd1: i64, rs1: i64, shamt: i64, fill: i8) -> Self {
        let mut i = Self::i(op, rd1, rs1, shamt);
        i.fill = fill;
        i
    }
}

/// Reads trit `lane` from a packed instruction.
#[must_use]
pub fn get_trit(word: u64, lane: usize) -> i8 {
    match (word >> (lane * 2)) & 0b11 {
        0b00 => 0,
        0b01 => 1,
        0b10 => -1,
        _ => 0,
    }
}

fn set_trit(word: &mut u64, lane: usize, trit: i8) {
    let code: u64 = match trit {
        1 => 0b01,
        -1 => 0b10,
        _ => 0b00,
    };
    let shift = lane * 2;
    *word &= !(0b11 << shift);
    *word |= code << shift;
}

/// Writes a balanced field of `width` trits at `offset`.
pub fn put_field(word: &mut u64, offset: usize, width: usize, value: i64) {
    let modulus = POW3[width];
    let half = (modulus - 1) / 2;
    let mut v = value % modulus;
    if v > half {
        v -= modulus;
    } else if v < -half {
        v += modulus;
    }
    for i in 0..width {
        let (trit, rest) = crate::rebel6::word::split_trit(v);
        set_trit(word, offset + i, trit);
        v = rest;
    }
}

/// Reads a balanced field of `width` trits at `offset`.
#[must_use]
pub fn get_field(word: u64, offset: usize, width: usize) -> i64 {
    let mut value = 0i64;
    for (i, &power) in POW3.iter().take(width).enumerate() {
        value += i64::from(get_trit(word, offset + i)) * power;
    }
    value
}

/// Returns the signed value of the low `width` trits of `value`.
fn balanced_low(value: i64, width: usize) -> i64 {
    let modulus = POW3[width];
    let half = (modulus - 1) / 2;
    let mut low = value.rem_euclid(modulus);
    if low > half {
        low -= modulus;
    }
    low
}

/// Encodes an instruction into its packed 32-trit form.
#[must_use]
pub fn encode(instr: &Instr) -> u64 {
    let (format, opcode, func) = spec(instr.op);
    let mut word = 0u64;
    match format {
        Format::R => {
            put_field(&mut word, 0, 4, opcode);
            put_field(&mut word, 4, 4, func);
            let rd2 = if instr.op == Op::ShT {
                i64::from(instr.fill)
            } else {
                instr.rd2
            };
            put_field(&mut word, 8, 6, rd2);
            put_field(&mut word, 14, 6, instr.rd1);
            put_field(&mut word, 20, 6, instr.rs2);
            put_field(&mut word, 26, 6, instr.rs1);
        }
        Format::I => {
            put_field(&mut word, 0, 4, opcode);
            put_field(&mut word, 4, 4, func);
            if instr.op == Op::ShiT {
                put_field(&mut word, 8, 6, instr.imm);
                put_field(&mut word, 14, 6, instr.rd1);
                put_field(&mut word, 20, 6, i64::from(instr.fill));
                put_field(&mut word, 26, 6, instr.rs1);
            } else {
                let low = balanced_low(instr.imm, 6);
                let high = (instr.imm - low) / POW3[6];
                put_field(&mut word, 8, 6, low);
                put_field(&mut word, 14, 6, instr.rd1);
                put_field(&mut word, 20, 6, high);
                put_field(&mut word, 26, 6, instr.rs1);
            }
        }
        Format::B => {
            put_field(&mut word, 0, 4, opcode);
            put_field(&mut word, 4, 4, func);
            if matches!(instr.op, Op::BcgsT | Op::BcegT) {
                put_field(&mut word, 8, 6, instr.imm2);
                put_field(&mut word, 14, 6, instr.imm);
            } else {
                put_field(&mut word, 8, 12, instr.imm);
            }
            put_field(&mut word, 20, 6, instr.rs2);
            put_field(&mut word, 26, 6, instr.rs1);
        }
        Format::G => {
            put_field(&mut word, 0, 2, opcode);
            let low = balanced_low(instr.imm, 12);
            let high = (instr.imm - low) / POW3[12];
            put_field(&mut word, 2, 12, low);
            put_field(&mut word, 14, 6, instr.rd1);
            put_field(&mut word, 20, 12, high);
        }
        Format::X => {
            put_field(&mut word, 0, 4, opcode);
            put_field(&mut word, 4, 4, func);
            put_field(&mut word, 8, 6, instr.rd2);
            put_field(&mut word, 14, 6, instr.rd1);
            put_field(&mut word, 20, 6, instr.imm2);
            put_field(&mut word, 26, 6, instr.imm);
        }
        Format::D => {
            put_field(&mut word, 0, 4, opcode);
            put_field(&mut word, 4, 4, func);
            put_field(&mut word, 8, 6, instr.rs3);
            put_field(&mut word, 14, 6, instr.rd1);
            put_field(&mut word, 20, 6, instr.rs2);
            put_field(&mut word, 26, 6, instr.rs1);
        }
        Format::Y => {
            put_field(&mut word, 0, 2, opcode);
            put_field(&mut word, 2, 24, instr.imm);
            put_field(&mut word, 26, 6, instr.rs1);
        }
    }
    word
}

/// Decodes a packed 32-trit instruction.
///
/// # Errors
///
/// Returns `None` for an opcode or func outside the implemented instruction set.
#[must_use]
pub fn decode(word: u64) -> Option<Instr> {
    let t0 = get_trit(word, 0);
    if t0 != 0 {
        let opc = get_field(word, 0, 2);
        let op = op_for_long_encoding(opc)?;
        let mut instr = Instr::new(op);
        if op == Op::SwaT {
            instr.imm = get_field(word, 2, 24);
            instr.rs1 = get_field(word, 26, 6);
        } else {
            let low = get_field(word, 2, 12);
            let high = get_field(word, 20, 12);
            instr.imm = low + high * POW3[12];
            instr.rd1 = get_field(word, 14, 6);
        }
        return Some(instr);
    }

    let opcode = get_field(word, 0, 4);
    let func = get_field(word, 4, 4);
    let op = op_for_encoding(opcode, func)?;
    let mut instr = Instr::new(op);
    match spec(op).0 {
        Format::R => {
            instr.rd2 = get_field(word, 8, 6);
            instr.rd1 = get_field(word, 14, 6);
            instr.rs2 = get_field(word, 20, 6);
            instr.rs1 = get_field(word, 26, 6);
            if op == Op::ShT {
                instr.fill = instr.rd2 as i8;
                instr.rd2 = 0;
            }
        }
        Format::I => {
            let low = get_field(word, 8, 6);
            instr.rd1 = get_field(word, 14, 6);
            let high = get_field(word, 20, 6);
            instr.rs1 = get_field(word, 26, 6);
            if op == Op::ShiT {
                instr.imm = low;
                instr.fill = high as i8;
            } else {
                instr.imm = low + high * POW3[6];
            }
        }
        Format::B => {
            if matches!(op, Op::BcgsT | Op::BcegT) {
                instr.imm2 = get_field(word, 8, 6);
                instr.imm = get_field(word, 14, 6);
            } else {
                instr.imm = get_field(word, 8, 12);
            }
            instr.rs2 = get_field(word, 20, 6);
            instr.rs1 = get_field(word, 26, 6);
        }
        Format::X => {
            instr.rd2 = get_field(word, 8, 6);
            instr.rd1 = get_field(word, 14, 6);
            instr.imm2 = get_field(word, 20, 6);
            instr.imm = get_field(word, 26, 6);
        }
        Format::D => {
            instr.rs3 = get_field(word, 8, 6);
            instr.rd1 = get_field(word, 14, 6);
            instr.rs2 = get_field(word, 20, 6);
            instr.rs1 = get_field(word, 26, 6);
        }
        Format::G | Format::Y => unreachable!("handled above"),
    }
    Some(instr)
}

/// Number of trits in an instruction word.
pub const INSTR_TRITS: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_and_func_constants_match_the_reference() {
        assert_eq!(spec(Op::AddT).1, -36);
        assert_eq!(spec(Op::AddT).2, -4);
        assert_eq!(spec(Op::SubT).2, -3);
        assert_eq!(spec(Op::AddiT).1, 0);
        assert_eq!(spec(Op::Addi).1, -3);
        assert_eq!(spec(Op::Add).1, -39);
        assert_eq!(spec(Op::LiT).1, 1);
        assert_eq!(spec(Op::LwaT).1, 4);
        assert_eq!(spec(Op::SwaT).1, -2);
        assert_eq!(spec(Op::JalT).1, 2);
        assert_eq!(spec(Op::AipcT).1, -1);
    }

    #[test]
    fn field_round_trip_handles_negative_values() {
        let mut word = 0u64;
        put_field(&mut word, 0, 12, -1000);
        assert_eq!(get_field(word, 0, 12), -1000);
        let mut word = 0u64;
        put_field(&mut word, 5, 24, -141_214_768_240);
        assert_eq!(get_field(word, 5, 24), -141_214_768_240);
    }

    #[test]
    fn instruction_width_is_32_trits() {
        let word = encode(&Instr::r(Op::AddT, 1, 2, 3));
        assert_eq!(get_trit(word, 31), 0);
        assert_eq!(get_field(word, 26, 6), 2);
    }
}
