// Temporary instrument: dump encode/decode for every op as ground truth for
// the Verilog decoder verification. Not part of the committed deliverable.
use setun_doom::rebel6::encoding::{decode, encode, Instr};
use setun_doom::rebel6::isa::Op;

fn main() {
    let ops = vec![
        Op::AddT, Op::SubT, Op::ShT, Op::RotrT, Op::SltT, Op::OrT, Op::XorT,
        Op::AndT, Op::CmpT, Op::StiT, Op::Mv2T, Op::MinT, Op::MaxT,
        Op::AddiT, Op::ShiT, Op::RotT, Op::SltiT, Op::OriT, Op::XoriT,
        Op::AndiT, Op::LwT, Op::LhT, Op::LtT, Op::JalrT, Op::BcgsT,
        Op::BcegT, Op::BeqT, Op::BneT, Op::BltT, Op::BgeT, Op::SwT,
        Op::ShTStore, Op::StT, Op::MajvT, Op::MinvT, Op::Li2T, Op::FenceT,
        Op::WfiT, Op::TretT, Op::EbreakT, Op::EcallT, Op::LwaT, Op::LiT,
        Op::SwaT, Op::JalT, Op::AipcT, Op::Add, Op::Sub, Op::Sll, Op::Srl,
        Op::Sra, Op::Sltu, Op::Or, Op::Xor, Op::And, Op::Addi, Op::Slli,
        Op::Srli, Op::Srai, Op::Sltiu, Op::Ori, Op::Xori, Op::Andi, Op::Lw,
        Op::Lh, Op::Lb, Op::Lhu, Op::Lbu, Op::Bltu, Op::Bgeu, Op::Sw,
        Op::Sh, Op::Sb,
    ];
    for (i, op) in ops.iter().enumerate() {
        let mut instr = Instr::new(*op);
        instr.rs1 = 17;
        instr.rs2 = 18;
        instr.rd1 = 19;
        instr.rd2 = 20;
        instr.rs3 = 21;
        instr.imm = 375;       // 3^5 + 3^4 + 3^3 + 3^2 + 3^1 + 3^0 forms a stable pattern
        instr.imm2 = 44;
        instr.fill = -1;
        let word = encode(&instr);
        let dec = decode(word).expect("must decode");
        println!(
            "{i}|{word:016x}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            dec.rd1, dec.rd2, dec.rs1, dec.rs2, dec.rs3, dec.imm, dec.imm2, dec.fill, 0
        );
    }
}
