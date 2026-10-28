//! 32-trit REBEL-6 instruction encoding.
//!
//! Fields are laid out most-significant-trit first as specified in the REBEL-6 reference:
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
//!
//! Instructions are stored packed at two bits per trit in a `u64`, lane 0 being the least
//! significant trit.

use crate::rebel6::word::POW3;

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

impl Op {
    /// Every architectural instruction implemented by this core.
    pub const ALL: &'static [Op] = &[
        Op::AddT,
        Op::SubT,
        Op::ShT,
        Op::RotrT,
        Op::SltT,
        Op::OrT,
        Op::XorT,
        Op::AndT,
        Op::CmpT,
        Op::StiT,
        Op::Mv2T,
        Op::MinT,
        Op::MaxT,
        Op::AddiT,
        Op::ShiT,
        Op::RotT,
        Op::SltiT,
        Op::OriT,
        Op::XoriT,
        Op::AndiT,
        Op::LwT,
        Op::LhT,
        Op::LtT,
        Op::JalrT,
        Op::BcgsT,
        Op::BcegT,
        Op::BeqT,
        Op::BneT,
        Op::BltT,
        Op::BgeT,
        Op::SwT,
        Op::ShTStore,
        Op::StT,
        Op::MajvT,
        Op::MinvT,
        Op::Li2T,
        Op::FenceT,
        Op::WfiT,
        Op::TretT,
        Op::EbreakT,
        Op::EcallT,
        Op::LwaT,
        Op::LiT,
        Op::SwaT,
        Op::JalT,
        Op::AipcT,
        Op::Add,
        Op::Sub,
        Op::Sll,
        Op::Srl,
        Op::Sra,
        Op::Sltu,
        Op::Or,
        Op::Xor,
        Op::And,
        Op::Addi,
        Op::Slli,
        Op::Srli,
        Op::Srai,
        Op::Sltiu,
        Op::Ori,
        Op::Xori,
        Op::Andi,
        Op::Lw,
        Op::Lh,
        Op::Lb,
        Op::Lhu,
        Op::Lbu,
        Op::Bltu,
        Op::Bgeu,
        Op::Sw,
        Op::Sh,
        Op::Sb,
    ];

    /// Mnemonic as written in `.tas` assembly.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Op::AddT => "add.t",
            Op::SubT => "sub.t",
            Op::ShT => "sh.t",
            Op::RotrT => "rotr.t",
            Op::SltT => "slt.t",
            Op::OrT => "or.t",
            Op::XorT => "xor.t",
            Op::AndT => "and.t",
            Op::CmpT => "cmp.t",
            Op::StiT => "sti.t",
            Op::Mv2T => "mv2.t",
            Op::MinT => "min.t",
            Op::MaxT => "max.t",
            Op::AddiT => "addi.t",
            Op::ShiT => "shi.t",
            Op::RotT => "rot.t",
            Op::SltiT => "slti.t",
            Op::OriT => "ori.t",
            Op::XoriT => "xori.t",
            Op::AndiT => "andi.t",
            Op::LwT => "lw.t",
            Op::LhT => "lh.t",
            Op::LtT => "lt.t",
            Op::JalrT => "jalr.t",
            Op::BcgsT => "bcgs.t",
            Op::BcegT => "bceg.t",
            Op::BeqT => "beq.t",
            Op::BneT => "bne.t",
            Op::BltT => "blt.t",
            Op::BgeT => "bge.t",
            Op::SwT => "sw.t",
            Op::ShTStore => "sh.t",
            Op::StT => "st.t",
            Op::MajvT => "majv.t",
            Op::MinvT => "minv.t",
            Op::Li2T => "li2.t",
            Op::FenceT => "fence.t",
            Op::WfiT => "wfi.t",
            Op::TretT => "tret.t",
            Op::EbreakT => "ebreak.t",
            Op::EcallT => "ecall.t",
            Op::LwaT => "lwa.t",
            Op::LiT => "li.t",
            Op::SwaT => "swa.t",
            Op::JalT => "jal.t",
            Op::AipcT => "aipc.t",
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Sll => "sll",
            Op::Srl => "srl",
            Op::Sra => "sra",
            Op::Sltu => "sltu",
            Op::Or => "or",
            Op::Xor => "xor",
            Op::And => "and",
            Op::Addi => "addi",
            Op::Slli => "slli",
            Op::Srli => "srli",
            Op::Srai => "srai",
            Op::Sltiu => "sltiu",
            Op::Ori => "ori",
            Op::Xori => "xori",
            Op::Andi => "andi",
            Op::Lw => "lw",
            Op::Lh => "lh",
            Op::Lb => "lb",
            Op::Lhu => "lhu",
            Op::Lbu => "lbu",
            Op::Bltu => "bltu",
            Op::Bgeu => "bgeu",
            Op::Sw => "sw",
            Op::Sh => "sh",
            Op::Sb => "sb",
        }
    }
}

const fn p4(a: i8, b: i8, c: i8, d: i8) -> i64 {
    ((a as i64 * 3 + b as i64) * 3 + c as i64) * 3 + d as i64
}

const fn p2(a: i8, b: i8) -> i64 {
    a as i64 * 3 + b as i64
}

/// Returns `(format, opcode, func)` for an instruction.
#[must_use]
pub const fn spec(op: Op) -> (Format, i64, i64) {
    use Format::{B, D, G, I, R, X, Y};
    match op {
        Op::AddT => (R, p4(-1, -1, 0, 0), p4(0, 0, -1, -1)),
        Op::SubT => (R, p4(-1, -1, 0, 0), p4(0, 0, -1, 0)),
        Op::ShT => (R, p4(-1, -1, 0, 0), p4(0, 0, -1, 1)),
        Op::RotrT => (R, p4(-1, -1, 0, 0), p4(0, 0, 0, -1)),
        Op::SltT => (R, p4(-1, -1, 0, 0), p4(0, 0, 0, 0)),
        Op::OrT => (R, p4(-1, -1, 0, 0), p4(0, 0, 0, 1)),
        Op::XorT => (R, p4(-1, -1, 0, 0), p4(0, 0, 1, -1)),
        Op::AndT => (R, p4(-1, -1, 0, 0), p4(0, 0, 1, 0)),
        Op::CmpT => (R, p4(-1, 0, 0, 0), p4(0, 0, -1, -1)),
        Op::StiT => (R, p4(-1, 0, 0, 0), p4(0, 0, -1, 0)),
        Op::Mv2T => (R, p4(-1, 0, 0, 0), p4(0, 0, -1, 1)),
        Op::MinT => (R, p4(-1, 0, 0, 0), p4(0, 0, 0, -1)),
        Op::MaxT => (R, p4(-1, 0, 0, 0), p4(0, 0, 0, 1)),
        Op::AddiT => (I, p4(0, 0, 0, 0), p4(0, 0, 0, 0)),
        Op::ShiT => (I, p4(0, 0, 0, 0), p4(0, 0, -1, -1)),
        Op::RotT => (I, p4(0, 0, 0, 0), p4(0, 0, -1, 1)),
        Op::SltiT => (I, p4(0, 0, 0, 0), p4(0, 0, 0, -1)),
        Op::OriT => (I, p4(0, 0, 0, 0), p4(0, 0, 0, 1)),
        Op::XoriT => (I, p4(0, 0, 0, 0), p4(0, 0, 1, -1)),
        Op::AndiT => (I, p4(0, 0, 0, 0), p4(0, 0, 1, 0)),
        Op::LwT => (I, p4(-1, 1, 0, 0), p4(0, 0, -1, -1)),
        Op::LhT => (I, p4(-1, 1, 0, 0), p4(0, 0, -1, 0)),
        Op::LtT => (I, p4(-1, 1, 0, 0), p4(0, 0, -1, 1)),
        Op::JalrT => (I, p4(-1, 1, 0, 0), p4(0, 0, 0, -1)),
        Op::BcgsT => (B, p4(0, -1, 0, 0), p4(0, 0, -1, -1)),
        Op::BcegT => (B, p4(0, -1, 0, 0), p4(0, 0, -1, 0)),
        Op::BeqT => (B, p4(0, -1, 0, 0), p4(0, 0, -1, 1)),
        Op::BneT => (B, p4(0, -1, 0, 0), p4(0, 0, 0, -1)),
        Op::BltT => (B, p4(0, -1, 0, 0), p4(0, 0, 0, 0)),
        Op::BgeT => (B, p4(0, -1, 0, 0), p4(0, 0, 0, 1)),
        Op::SwT => (B, p4(0, 1, 0, 0), p4(0, 0, -1, -1)),
        Op::ShTStore => (B, p4(0, 1, 0, 0), p4(0, 0, -1, 0)),
        Op::StT => (B, p4(0, 1, 0, 0), p4(0, 0, -1, 1)),
        Op::MajvT => (D, p4(1, -1, 0, 0), p4(0, 0, -1, -1)),
        Op::MinvT => (D, p4(1, -1, 0, 0), p4(0, 0, -1, 0)),
        Op::Li2T => (X, p4(1, 0, 0, 0), p4(0, 0, -1, -1)),
        Op::FenceT => (R, p4(1, 1, 0, 0), p4(0, 0, -1, 0)),
        Op::WfiT => (R, p4(1, 1, 0, 0), p4(0, 0, -1, 1)),
        Op::TretT => (R, p4(1, 1, 0, 0), p4(0, 0, 1, -1)),
        Op::EbreakT => (R, p4(1, 1, 0, 0), p4(0, 0, 1, 0)),
        Op::EcallT => (R, p4(1, 1, 0, 0), p4(0, 0, 1, 1)),
        Op::LwaT => (G, p2(1, 1), 0),
        Op::LiT => (G, p2(0, 1), 0),
        Op::SwaT => (Y, p2(-1, 1), 0),
        Op::JalT => (G, p2(1, -1), 0),
        Op::AipcT => (G, p2(0, -1), 0),
        Op::Add => (R, p4(-1, -1, -1, 0), p4(0, 0, -1, -1)),
        Op::Sub => (R, p4(-1, -1, -1, 0), p4(0, 0, -1, 0)),
        Op::Sll => (R, p4(-1, -1, -1, 0), p4(0, 0, -1, 1)),
        Op::Srl => (R, p4(-1, -1, -1, 0), p4(0, 0, 0, -1)),
        Op::Sra => (R, p4(-1, -1, -1, 0), p4(0, 0, 0, 0)),
        Op::Sltu => (R, p4(-1, -1, -1, 0), p4(0, 0, 0, 1)),
        Op::Or => (R, p4(-1, -1, -1, 0), p4(0, 0, 1, -1)),
        Op::Xor => (R, p4(-1, -1, -1, 0), p4(0, 0, 1, 0)),
        Op::And => (R, p4(-1, -1, -1, 0), p4(0, 0, 1, 1)),
        Op::Addi => (I, p4(0, 0, -1, 0), p4(0, 0, -1, -1)),
        Op::Slli => (I, p4(0, 0, -1, 0), p4(0, 0, -1, 0)),
        Op::Srli => (I, p4(0, 0, -1, 0), p4(0, 0, -1, 1)),
        Op::Srai => (I, p4(0, 0, -1, 0), p4(0, 0, 0, -1)),
        Op::Sltiu => (I, p4(0, 0, -1, 0), p4(0, 0, 0, 0)),
        Op::Ori => (I, p4(0, 0, -1, 0), p4(0, 0, 0, 1)),
        Op::Xori => (I, p4(0, 0, -1, 0), p4(0, 0, 1, -1)),
        Op::Andi => (I, p4(0, 0, -1, 0), p4(0, 0, 1, 0)),
        Op::Lw => (I, p4(-1, 1, -1, 0), p4(0, 0, -1, -1)),
        Op::Lh => (I, p4(-1, 1, -1, 0), p4(0, 0, -1, 0)),
        Op::Lb => (I, p4(-1, 1, -1, 0), p4(0, 0, -1, 1)),
        Op::Lhu => (I, p4(-1, 1, -1, 0), p4(0, 0, 0, -1)),
        Op::Lbu => (I, p4(-1, 1, -1, 0), p4(0, 0, 0, 0)),
        Op::Bltu => (B, p4(0, -1, -1, 0), p4(0, 0, -1, -1)),
        Op::Bgeu => (B, p4(0, -1, -1, 0), p4(0, 0, -1, 0)),
        Op::Sw => (B, p4(0, 1, -1, 0), p4(0, 0, -1, -1)),
        Op::Sh => (B, p4(0, 1, -1, 0), p4(0, 0, -1, 0)),
        Op::Sb => (B, p4(0, 1, -1, 0), p4(0, 0, -1, 1)),
    }
}

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
        let op = match opc {
            4 => Op::LwaT,
            1 => Op::LiT,
            -2 => Op::SwaT,
            2 => Op::JalT,
            -1 => Op::AipcT,
            _ => return None,
        };
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
    let op = match (opcode, func) {
        // Base Ternary R-ALU
        (-36, -4) => Op::AddT,
        (-36, -3) => Op::SubT,
        (-36, -2) => Op::ShT,
        (-36, -1) => Op::RotrT,
        (-36, 0) => Op::SltT,
        (-36, 1) => Op::OrT,
        (-36, 2) => Op::XorT,
        (-36, 3) => Op::AndT,
        // Base Ternary Compare/Unary
        (-27, -4) => Op::CmpT,
        (-27, -3) => Op::StiT,
        (-27, -2) => Op::Mv2T,
        (-27, -1) => Op::MinT,
        (-27, 1) => Op::MaxT,
        // Base Ternary I-ALU
        (0, -4) => Op::ShiT,
        (0, -2) => Op::RotT,
        (0, -1) => Op::SltiT,
        (0, 0) => Op::AddiT,
        (0, 1) => Op::OriT,
        (0, 2) => Op::XoriT,
        (0, 3) => Op::AndiT,
        // Base Ternary Load
        (-18, -4) => Op::LwT,
        (-18, -3) => Op::LhT,
        (-18, -2) => Op::LtT,
        (-18, -1) => Op::JalrT,
        // Base Ternary Branch
        (-9, -4) => Op::BcgsT,
        (-9, -3) => Op::BcegT,
        (-9, -2) => Op::BeqT,
        (-9, -1) => Op::BneT,
        (-9, 0) => Op::BltT,
        (-9, 1) => Op::BgeT,
        // Base Ternary Store
        (9, -4) => Op::SwT,
        (9, -3) => Op::ShTStore,
        (9, -2) => Op::StT,
        // Base Ternary D
        (18, -4) => Op::MajvT,
        (18, -3) => Op::MinvT,
        // Base Ternary X
        (27, -4) => Op::Li2T,
        // Base Ternary System
        (36, -3) => Op::FenceT,
        (36, -2) => Op::WfiT,
        (36, 2) => Op::TretT,
        (36, 3) => Op::EbreakT,
        (36, 4) => Op::EcallT,
        // Base Binary R-ALU
        (-39, -4) => Op::Add,
        (-39, -3) => Op::Sub,
        (-39, -2) => Op::Sll,
        (-39, -1) => Op::Srl,
        (-39, 0) => Op::Sra,
        (-39, 1) => Op::Sltu,
        (-39, 2) => Op::Or,
        (-39, 3) => Op::Xor,
        (-39, 4) => Op::And,
        // Base Binary I-ALU
        (-3, -4) => Op::Addi,
        (-3, -3) => Op::Slli,
        (-3, -2) => Op::Srli,
        (-3, -1) => Op::Srai,
        (-3, 0) => Op::Sltiu,
        (-3, 1) => Op::Ori,
        (-3, 2) => Op::Xori,
        (-3, 3) => Op::Andi,
        // Base Binary Load
        (-21, -4) => Op::Lw,
        (-21, -3) => Op::Lh,
        (-21, -2) => Op::Lb,
        (-21, -1) => Op::Lhu,
        (-21, 0) => Op::Lbu,
        // Base Binary Branch
        (-12, -4) => Op::Bltu,
        (-12, -3) => Op::Bgeu,
        // Base Binary Store
        (6, -4) => Op::Sw,
        (6, -3) => Op::Sh,
        (6, -2) => Op::Sb,
        _ => return None,
    };

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
    fn encode_decode_round_trip_every_op() {
        for &op in Op::ALL {
            let (format, _, _) = spec(op);
            let mut instr = Instr::new(op);
            match format {
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
            let word = encode(&instr);
            let decoded = decode(word).unwrap_or_else(|| panic!("failed to decode {}", op.name()));
            assert_eq!(decoded.op, op, "{}", op.name());
            assert_eq!(decoded, instr, "{}", op.name());
        }
    }

    #[test]
    fn instruction_width_is_32_trits() {
        let word = encode(&Instr::r(Op::AddT, 1, 2, 3));
        assert_eq!(get_trit(word, 31), 0);
        assert_eq!(get_field(word, 26, 6), 2);
    }
}
