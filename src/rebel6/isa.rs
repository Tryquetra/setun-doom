//! Single source of the REBEL-6 instruction set.
//!
//! Every architectural instruction is one row of [`INSTRUCTIONS`]: operation, mnemonic, format,
//! opcode and function field. The encoder, the decoder, the assembler mnemonic table and the
//! conformance tests read this table, so adding an instruction is a single edit.
//!
//! Formats, most significant trit first:
//!
//! ```text
//! R: rs1[6] | rs2[6] | rd1[6] | rd2[6] | func[4] | opcode[4]
//! I: rs1[6] | imm[11:6][6] | rd1[6] | imm[5:0][6] | func[4] | opcode[4]
//! B: rs1[6] | rs2[6] | imm[11:0][12] | func[4] | opcode[4]
//! G: imm[23:12][12] | rd1[6] | imm[11:0][12] | opc[2]
//! X: imm6a[6] | imm6b[6] | rd1[6] | rd2[6] | func[4] | opcode[4]
//! D: rs1[6] | rs2[6] | rd1[6] | rs3[6] | func[4] | opcode[4]
//! Y: rs1[6] | imm[23:0][24] | opc[2]
//! ```

/// Instruction formats.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    /// Register-register.
    R,
    /// Register-immediate.
    I,
    /// Branch.
    B,
    /// Long immediate (load, `li`, `jal`, `aipc`).
    G,
    /// Two immediates.
    X,
    /// Three-source.
    D,
    /// Absolute store.
    Y,
}

/// REBEL-6 architectural instruction.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
    // Base Ternary
    AddT,
    SubT,
    ShT,
    RotrT,
    SltT,
    OrT,
    XorT,
    AndT,
    CmpT,
    StiT,
    Mv2T,
    MinT,
    MaxT,
    AddiT,
    ShiT,
    RotT,
    SltiT,
    OriT,
    XoriT,
    AndiT,
    LwT,
    LhT,
    LtT,
    JalrT,
    BcgsT,
    BcegT,
    BeqT,
    BneT,
    BltT,
    BgeT,
    SwT,
    ShTStore,
    StT,
    MajvT,
    MinvT,
    Li2T,
    FenceT,
    WfiT,
    TretT,
    EbreakT,
    EcallT,
    LwaT,
    LiT,
    SwaT,
    JalT,
    AipcT,
    // Base Binary (RV32I compatibility)
    Add,
    Sub,
    Sll,
    Srl,
    Sra,
    Sltu,
    Or,
    Xor,
    And,
    Addi,
    Slli,
    Srli,
    Srai,
    Sltiu,
    Ori,
    Xori,
    Andi,
    Lw,
    Lh,
    Lb,
    Lhu,
    Lbu,
    Bltu,
    Bgeu,
    Sw,
    Sh,
    Sb,
}

/// One row of the instruction set.
#[derive(Copy, Clone, Debug)]
pub struct Spec {
    /// Architectural operation.
    pub op: Op,
    /// Mnemonic as written in `.tas` assembly.
    pub name: &'static str,
    /// Instruction format.
    pub format: Format,
    /// Opcode field.
    pub opcode: i64,
    /// Function field, zero when the format has none.
    pub func: i64,
}

const fn p4(a: i8, b: i8, c: i8, d: i8) -> i64 {
    ((a as i64 * 3 + b as i64) * 3 + c as i64) * 3 + d as i64
}

const fn p2(a: i8, b: i8) -> i64 {
    a as i64 * 3 + b as i64
}

/// Every architectural instruction implemented by this core.
pub const INSTRUCTIONS: &[Spec] = &[
    Spec { op: Op::AddT, name: "add.t", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::SubT, name: "sub.t", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, -1, 0) },
    // `sh.t` names the B-format store; the R-format shift is spelled through its fill aliases.
    Spec { op: Op::ShT, name: "", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::RotrT, name: "rotr.t", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::SltT, name: "slt.t", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, 0, 0) },
    Spec { op: Op::OrT, name: "or.t", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, 0, 1) },
    Spec { op: Op::XorT, name: "xor.t", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, 1, -1) },
    Spec { op: Op::AndT, name: "and.t", format: Format::R, opcode: p4(-1, -1, 0, 0), func: p4(0, 0, 1, 0) },
    Spec { op: Op::CmpT, name: "cmp.t", format: Format::R, opcode: p4(-1, 0, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::StiT, name: "sti.t", format: Format::R, opcode: p4(-1, 0, 0, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::Mv2T, name: "mv2.t", format: Format::R, opcode: p4(-1, 0, 0, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::MinT, name: "min.t", format: Format::R, opcode: p4(-1, 0, 0, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::MaxT, name: "max.t", format: Format::R, opcode: p4(-1, 0, 0, 0), func: p4(0, 0, 0, 1) },
    Spec { op: Op::AddiT, name: "addi.t", format: Format::I, opcode: p4(0, 0, 0, 0), func: p4(0, 0, 0, 0) },
    Spec { op: Op::ShiT, name: "shi.t", format: Format::I, opcode: p4(0, 0, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::RotT, name: "rot.t", format: Format::I, opcode: p4(0, 0, 0, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::SltiT, name: "slti.t", format: Format::I, opcode: p4(0, 0, 0, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::OriT, name: "ori.t", format: Format::I, opcode: p4(0, 0, 0, 0), func: p4(0, 0, 0, 1) },
    Spec { op: Op::XoriT, name: "xori.t", format: Format::I, opcode: p4(0, 0, 0, 0), func: p4(0, 0, 1, -1) },
    Spec { op: Op::AndiT, name: "andi.t", format: Format::I, opcode: p4(0, 0, 0, 0), func: p4(0, 0, 1, 0) },
    Spec { op: Op::LwT, name: "lw.t", format: Format::I, opcode: p4(-1, 1, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::LhT, name: "lh.t", format: Format::I, opcode: p4(-1, 1, 0, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::LtT, name: "lt.t", format: Format::I, opcode: p4(-1, 1, 0, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::JalrT, name: "jalr.t", format: Format::I, opcode: p4(-1, 1, 0, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::BcgsT, name: "bcgs.t", format: Format::B, opcode: p4(0, -1, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::BcegT, name: "bceg.t", format: Format::B, opcode: p4(0, -1, 0, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::BeqT, name: "beq.t", format: Format::B, opcode: p4(0, -1, 0, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::BneT, name: "bne.t", format: Format::B, opcode: p4(0, -1, 0, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::BltT, name: "blt.t", format: Format::B, opcode: p4(0, -1, 0, 0), func: p4(0, 0, 0, 0) },
    Spec { op: Op::BgeT, name: "bge.t", format: Format::B, opcode: p4(0, -1, 0, 0), func: p4(0, 0, 0, 1) },
    Spec { op: Op::SwT, name: "sw.t", format: Format::B, opcode: p4(0, 1, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::ShTStore, name: "sh.t", format: Format::B, opcode: p4(0, 1, 0, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::StT, name: "st.t", format: Format::B, opcode: p4(0, 1, 0, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::MajvT, name: "majv.t", format: Format::D, opcode: p4(1, -1, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::MinvT, name: "minv.t", format: Format::D, opcode: p4(1, -1, 0, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::Li2T, name: "li2.t", format: Format::X, opcode: p4(1, 0, 0, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::FenceT, name: "fence.t", format: Format::R, opcode: p4(1, 1, 0, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::WfiT, name: "wfi.t", format: Format::R, opcode: p4(1, 1, 0, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::TretT, name: "tret.t", format: Format::R, opcode: p4(1, 1, 0, 0), func: p4(0, 0, 1, -1) },
    Spec { op: Op::EbreakT, name: "ebreak.t", format: Format::R, opcode: p4(1, 1, 0, 0), func: p4(0, 0, 1, 0) },
    Spec { op: Op::EcallT, name: "ecall.t", format: Format::R, opcode: p4(1, 1, 0, 0), func: p4(0, 0, 1, 1) },
    Spec { op: Op::LwaT, name: "lwa.t", format: Format::G, opcode: p2(1, 1), func: 0 },
    Spec { op: Op::LiT, name: "li.t", format: Format::G, opcode: p2(0, 1), func: 0 },
    Spec { op: Op::SwaT, name: "swa.t", format: Format::Y, opcode: p2(-1, 1), func: 0 },
    Spec { op: Op::JalT, name: "jal.t", format: Format::G, opcode: p2(1, -1), func: 0 },
    Spec { op: Op::AipcT, name: "aipc.t", format: Format::G, opcode: p2(0, -1), func: 0 },
    Spec { op: Op::Add, name: "add", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::Sub, name: "sub", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::Sll, name: "sll", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::Srl, name: "srl", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::Sra, name: "sra", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, 0, 0) },
    Spec { op: Op::Sltu, name: "sltu", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, 0, 1) },
    Spec { op: Op::Or, name: "or", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, 1, -1) },
    Spec { op: Op::Xor, name: "xor", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, 1, 0) },
    Spec { op: Op::And, name: "and", format: Format::R, opcode: p4(-1, -1, -1, 0), func: p4(0, 0, 1, 1) },
    Spec { op: Op::Addi, name: "addi", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::Slli, name: "slli", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::Srli, name: "srli", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::Srai, name: "srai", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::Sltiu, name: "sltiu", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, 0, 0) },
    Spec { op: Op::Ori, name: "ori", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, 0, 1) },
    Spec { op: Op::Xori, name: "xori", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, 1, -1) },
    Spec { op: Op::Andi, name: "andi", format: Format::I, opcode: p4(0, 0, -1, 0), func: p4(0, 0, 1, 0) },
    Spec { op: Op::Lw, name: "lw", format: Format::I, opcode: p4(-1, 1, -1, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::Lh, name: "lh", format: Format::I, opcode: p4(-1, 1, -1, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::Lb, name: "lb", format: Format::I, opcode: p4(-1, 1, -1, 0), func: p4(0, 0, -1, 1) },
    Spec { op: Op::Lhu, name: "lhu", format: Format::I, opcode: p4(-1, 1, -1, 0), func: p4(0, 0, 0, -1) },
    Spec { op: Op::Lbu, name: "lbu", format: Format::I, opcode: p4(-1, 1, -1, 0), func: p4(0, 0, 0, 0) },
    Spec { op: Op::Bltu, name: "bltu", format: Format::B, opcode: p4(0, -1, -1, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::Bgeu, name: "bgeu", format: Format::B, opcode: p4(0, -1, -1, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::Sw, name: "sw", format: Format::B, opcode: p4(0, 1, -1, 0), func: p4(0, 0, -1, -1) },
    Spec { op: Op::Sh, name: "sh", format: Format::B, opcode: p4(0, 1, -1, 0), func: p4(0, 0, -1, 0) },
    Spec { op: Op::Sb, name: "sb", format: Format::B, opcode: p4(0, 1, -1, 0), func: p4(0, 0, -1, 1) },
];

/// Returns `(format, opcode, func)` for an instruction.
#[must_use]
pub fn spec(op: Op) -> (Format, i64, i64) {
    let row = row(op);
    (row.format, row.opcode, row.func)
}

/// Returns the table row of an instruction.
#[must_use]
pub fn row(op: Op) -> &'static Spec {
    INSTRUCTIONS
        .iter()
        .find(|row| row.op == op)
        .expect("every instruction has a table row")
}

/// Mnemonic as written in `.tas` assembly.
#[must_use]
pub fn name(op: Op) -> &'static str {
    row(op).name
}

/// Resolves a canonical mnemonic to its instruction.
#[must_use]
pub fn op_for_name(name: &str) -> Option<Op> {
    if name.is_empty() {
        return None;
    }
    INSTRUCTIONS
        .iter()
        .find(|row| row.name == name)
        .map(|row| row.op)
}

/// Resolves an opcode and function field to its instruction, for the 4-trit formats.
#[must_use]
pub fn op_for_encoding(opcode: i64, func: i64) -> Option<Op> {
    INSTRUCTIONS
        .iter()
        .find(|row| !matches!(row.format, Format::G | Format::Y) && row.opcode == opcode && row.func == func)
        .map(|row| row.op)
}

/// Resolves a 2-trit opcode to its instruction, for the `G` and `Y` formats.
#[must_use]
pub fn op_for_long_encoding(opcode: i64) -> Option<Op> {
    INSTRUCTIONS
        .iter()
        .find(|row| matches!(row.format, Format::G | Format::Y) && row.opcode == opcode)
        .map(|row| row.op)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebel6::encoding::{decode, encode, Instr};

    #[test]
    fn every_encoding_is_unique() {
        for (index, row) in INSTRUCTIONS.iter().enumerate() {
            for other in &INSTRUCTIONS[index + 1..] {
                let same_long = matches!(row.format, Format::G | Format::Y)
                    && matches!(other.format, Format::G | Format::Y)
                    && row.opcode == other.opcode;
                let same_short = !matches!(row.format, Format::G | Format::Y)
                    && !matches!(other.format, Format::G | Format::Y)
                    && row.opcode == other.opcode
                    && row.func == other.func;
                assert!(!same_long && !same_short, "{} and {} share an encoding", row.name, other.name);
                if !row.name.is_empty() {
                    assert_ne!(row.name, other.name, "duplicate mnemonic {}", row.name);
                }
            }
        }
    }

    #[test]
    fn mnemonics_resolve_to_their_instruction() {
        for row in INSTRUCTIONS {
            if !row.name.is_empty() {
                assert_eq!(op_for_name(row.name), Some(row.op), "{}", row.name);
            }
            assert_eq!(name(row.op), row.name);
            assert_eq!(spec(row.op), (row.format, row.opcode, row.func));
        }
    }

    #[test]
    fn every_instruction_encodes_and_decodes() {
        for row in INSTRUCTIONS {
            let op = row.op;
            let mut instr = Instr::new(op);
            match row.format {
                Format::R => {
                    instr.rd1 = 3;
                    instr.rs1 = 5;
                    instr.rs2 = 7;
                    if op == Op::ShT {
                        instr.fill = -1;
                    }
                }
                Format::I => {
                    instr.rd1 = 3;
                    instr.rs1 = 5;
                    instr.imm = if op == Op::ShiT { -7 } else { 1000 };
                    if op == Op::ShiT {
                        instr.fill = 1;
                    }
                }
                Format::B => {
                    instr.rs1 = 3;
                    instr.rs2 = 5;
                    if matches!(op, Op::BcgsT | Op::BcegT) {
                        instr.imm = 10;
                        instr.imm2 = -10;
                    } else {
                        instr.imm = 1000;
                    }
                }
                Format::G => {
                    instr.rd1 = 3;
                    instr.imm = 123_456;
                }
                Format::X => {
                    instr.rd1 = 3;
                    instr.rd2 = 5;
                    instr.imm = 6;
                    instr.imm2 = -6;
                }
                Format::D => {
                    instr.rd1 = 3;
                    instr.rs1 = 5;
                    instr.rs2 = 7;
                    instr.rs3 = 9;
                }
                Format::Y => {
                    instr.rs1 = 3;
                    instr.imm = 123_456;
                }
            }
            let decoded = decode(encode(&instr)).unwrap_or_else(|| panic!("failed to decode {}", row.name));
            assert_eq!(decoded, instr, "{}", row.name);
        }
    }
}
