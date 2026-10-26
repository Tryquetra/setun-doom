//! # Montador Textual de Duas Passagens (`SetunAssembler`) e Disassembler
//!
//! Traduz código-fonte assembly da arquitetura **Setun-Doom** com suporte a:
//! - Labels simbólicos (`.loop:`, `render_col:`).
//! - Registradores de índice (`IX0`..`IX8`).
//! - Literais decimais (`-42`, `4600000`) e literais ternários balanceados (`0t+0-+0`).
//! - Comentários iniciados por `;` ou `#`.

use crate::cpu::{Instruction, Opcode, INDEX_REGISTER_COUNT};
use crate::word::Word21;
use std::collections::HashMap;

/// Erro de montagem com número da linha e mensagem explicativa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsmError {
    pub line: usize,
    pub message: String,
}

/// Montador de duas passagens para a ISA Setun-Doom.
pub struct SetunAssembler;

impl SetunAssembler {
    /// Monta um programa em assembly textual Setun-Doom em um vetor de [`Instruction`].
    pub fn assemble(source: &str) -> Result<Vec<Instruction>, AsmError> {
        Self::assemble_at(source, 0)
    }

    /// Monta um programa posicionando os endereços de labels a partir de `base_addr`.
    pub fn assemble_at(source: &str, base_addr: i64) -> Result<Vec<Instruction>, AsmError> {
        let mut labels: HashMap<String, i64> = HashMap::new();
        let mut cleaned_lines: Vec<(usize, String)> = Vec::new();
        let mut pc: i64 = base_addr;

        // 1ª Passagem: Coleta de labels e contagem de endereços (PC)
        for (idx, raw_line) in source.lines().enumerate() {
            let line_num = idx + 1;
            let without_comment = raw_line
                .split([';', '#'])
                .next()
                .unwrap_or("")
                .trim();

            if without_comment.is_empty() {
                continue;
            }

            if let Some(label_part) = without_comment.strip_suffix(':') {
                let label_name = label_part.trim().to_string();
                if label_name.is_empty() {
                    return Err(AsmError {
                        line: line_num,
                        message: "Label vazio".to_string(),
                    });
                }
                labels.insert(label_name, pc);
                continue;
            }

            // Suporta label na mesma linha da instrução: "loop: ADD 100"
            if let Some((lbl, rest)) = without_comment.split_once(':') {
                let label_name = lbl.trim().to_string();
                labels.insert(label_name, pc);
                let instr_str = rest.trim();
                if !instr_str.is_empty() {
                    cleaned_lines.push((line_num, instr_str.to_string()));
                    pc += 1;
                }
            } else {
                cleaned_lines.push((line_num, without_comment.to_string()));
                pc += 1;
            }
        }

        // 2ª Passagem: Emissão de instruções e resolução de símbolos
        let mut program = Vec::with_capacity(cleaned_lines.len());
        for (offset, (line_num, line)) in cleaned_lines.into_iter().enumerate() {
            let instr_pc = base_addr + (offset as i64);
            let instr = Self::parse_instruction_line(line_num, instr_pc, &line, &labels)?;
            program.push(instr);
        }

        Ok(program)
    }

    fn parse_instruction_line(
        line_num: usize,
        instr_pc: i64,
        line: &str,
        labels: &HashMap<String, i64>,
    ) -> Result<Instruction, AsmError> {
        let normalized = line.replace(',', " ");
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(AsmError {
                line: line_num,
                message: "Linha de instrucao vazia".to_string(),
            });
        }

        let mnemonic = tokens[0].to_uppercase();
        let (opcode, mut reg_idx) = Self::parse_mnemonic(&mnemonic).ok_or_else(|| AsmError {
            line: line_num,
            message: format!("Mnemonico desconhecido: '{}'", tokens[0]),
        })?;

        let mut operand: i64 = 0;

        if opcode == Opcode::Bceg {
            // Sintaxe:
            // BCEG IX0, target_eq, target_gt
            // ou BCEG target_eq, target_gt (usa IX0 por padrão)
            let (eq_tok, gt_tok) = match tokens.len() {
                3 => (tokens[1], tokens[2]),
                4 => {
                    reg_idx = Self::parse_index_reg(tokens[1]).ok_or_else(|| AsmError {
                        line: line_num,
                        message: format!("Registrador de indice invalido em BCEG: '{}'", tokens[1]),
                    })?;
                    (tokens[2], tokens[3])
                }
                _ => {
                    return Err(AsmError {
                        line: line_num,
                        message: "BCEG espera: BCEG [IXn,] <alvo_eq>, <alvo_gt>".to_string(),
                    });
                }
            };
            let off_eq = Self::parse_relative_or_offset(line_num, instr_pc, eq_tok, labels)?;
            let off_gt = Self::parse_relative_or_offset(line_num, instr_pc, gt_tok, labels)?;
            operand = Instruction::pack_bceg_offsets(off_eq, off_gt);
            return Ok(Instruction::new(Opcode::Bceg, reg_idx, operand));
        }

        match tokens.len() {
            1 => {}
            2 => {
                if let Some(ix) = Self::parse_index_reg(tokens[1]) {
                    reg_idx = ix;
                } else {
                    operand = Self::parse_operand(line_num, tokens[1], labels)?;
                }
            }
            3 => {
                reg_idx = Self::parse_index_reg(tokens[1]).ok_or_else(|| AsmError {
                    line: line_num,
                    message: format!("Registrador de indice invalido: '{}'", tokens[1]),
                })?;
                operand = Self::parse_operand(line_num, tokens[2], labels)?;
            }
            _ => {
                return Err(AsmError {
                    line: line_num,
                    message: "Excesso de operandos na instrucao".to_string(),
                });
            }
        }

        Ok(Instruction::new(opcode, reg_idx, operand))
    }

    fn parse_mnemonic(m: &str) -> Option<(Opcode, usize)> {
        match m {
            "NOP" => Some((Opcode::Nop, 0)),
            "ADD" => Some((Opcode::Add, 0)),
            "SUB" => Some((Opcode::Sub, 0)),
            "MUL" | "MUL_ZERO" | "L*T" => Some((Opcode::Mul, 0)),
            "MUL_REM" => Some((Opcode::Mul, 1)),
            "DIV" => Some((Opcode::Div, 0)),
            "NEG" | "STI_T" | "STI.T" => Some((Opcode::Neg, 0)),
            "LOAD" => Some((Opcode::Load, 0)),
            "STORE" => Some((Opcode::Store, 0)),
            "LOAD_INDEX" | "LDX" => Some((Opcode::LoadIndex, 0)),
            "STORE_INDEX" | "STX" => Some((Opcode::StoreIndex, 0)),
            "JMP" => Some((Opcode::Jmp, 0)),
            "JMPN" | "JMP_N" | "BLTZ" | "BLT.T" => Some((Opcode::JmpN, 0)),
            "JMPGE" | "JMP_GE" | "BGEZ" | "BGE.T" => Some((Opcode::JmpN, 1)),
            "JMPZ" | "JMP_Z" | "BEQZ" | "BEQ.T" => Some((Opcode::JmpZ, 0)),
            "JMPNE" | "JMP_NE" | "BNEZ" | "BNE.T" => Some((Opcode::JmpZ, 1)),
            "JMPP" | "JMP_P" | "BGTZ" | "BGT.T" => Some((Opcode::JmpP, 0)),
            "JMPLE" | "JMP_LE" | "BLEZ" | "BLE.T" => Some((Opcode::JmpP, 1)),
            "BCEG" | "BCEG.T" => Some((Opcode::Bceg, 0)),
            "PUSH" => Some((Opcode::Push, 0)),
            "POP" => Some((Opcode::Pop, 0)),
            "CALL" => Some((Opcode::Call, 0)),
            "RET" => Some((Opcode::Ret, 0)),
            "LOAD_IMM" | "LDI" => Some((Opcode::LoadImm, 0)),
            "ADD_IMM" | "ADDI" => Some((Opcode::AddImm, 0)),
            "LOAD_IX_IMM" | "LDXI" => Some((Opcode::LoadIxImm, 0)),
            "ADD_IX_IMM" | "ADDXI" => Some((Opcode::AddIxImm, 0)),
            "MOV_ACC_IX" | "TAX" => Some((Opcode::MovAccToIx, 0)),
            "MOV_ACC_R" | "TAR" => Some((Opcode::MovAccToIx, 8)),
            "MOV_IX_ACC" | "TXA" => Some((Opcode::MovIxToAcc, 0)),
            "MOV_R_ACC" | "TRA" => Some((Opcode::MovIxToAcc, 8)),
            "SHIFT" | "SHF" | "SHF_IX_ACC" | "SHIFT_IX_ACC" => Some((Opcode::Shift, 0)),
            "MUL_INT" | "MULI" => Some((Opcode::MulInt, 0)),
            "DIV_INT" | "DIVI" => Some((Opcode::DivInt, 0)),
            "CMP_T" | "CMP.T" | "CMPT" => Some((Opcode::CmpT, 0)),
            "MAJV_T" | "MAJV.T" | "MAJVT" => Some((Opcode::MajvT, 0)),
            "MINV_T" | "MINV.T" | "MINVT" => Some((Opcode::MinvT, 0)),
            "ADD_IX" | "ADDIX" => Some((Opcode::AddIx, 0)),
            "BIX" | "DEC_IX_JGE" => Some((Opcode::DecIxJmpGe, 0)),
            "LOAD_R" | "YRT" => Some((Opcode::LoadR, 0)),
            "MAC_POS" | "LBT" => Some((Opcode::MacPos, 0)),
            "MAC_NEG" => Some((Opcode::MacNeg, 0)),
            "SMT" => Some((Opcode::Smt, 0)),
            "SAT" => Some((Opcode::Sat, 0)),
            "SIT" => Some((Opcode::Sit, 0)),
            "NORM_T" | "NORM.T" | "NORMT" | "CLZ_T" | "CLZ.T" | "CLZT" => Some((Opcode::NormT, 0)),
            "HALT" | "HLT" => Some((Opcode::Halt, 0)),
            _ => None,
        }
    }

    fn parse_index_reg(token: &str) -> Option<usize> {
        let up = token.to_uppercase();
        if let Some(num_str) = up.strip_prefix("IX") {
            let idx: usize = num_str.parse().ok()?;
            if idx < INDEX_REGISTER_COUNT {
                return Some(idx);
            }
        }
        None
    }

    fn parse_operand(
        line_num: usize,
        token: &str,
        labels: &HashMap<String, i64>,
    ) -> Result<i64, AsmError> {
        if let Some(&addr) = labels.get(token) {
            return Ok(addr);
        }
        if let Some(trit_lit) = token.strip_prefix("0t").or_else(|| token.strip_prefix("0T")) {
            let w = Word21::from_balanced_str(trit_lit).map_err(|e| AsmError {
                line: line_num,
                message: e.to_string(),
            })?;
            return Ok(w.as_i64());
        }
        token.parse::<i64>().map_err(|_| AsmError {
            line: line_num,
            message: format!("Operando ou label invalido: '{}'", token),
        })
    }

    fn parse_relative_or_offset(
        line_num: usize,
        instr_pc: i64,
        token: &str,
        labels: &HashMap<String, i64>,
    ) -> Result<i64, AsmError> {
        if token == "0" || token.eq_ignore_ascii_case("x0") {
            return Ok(0);
        }
        if let Some(&target_addr) = labels.get(token) {
            return Ok(target_addr - instr_pc);
        }
        Self::parse_operand(line_num, token, labels)
    }

    /// Formata uma [`Instruction`] de volta para sua representação textual legível (Disassembler).
    pub fn disassemble_instruction(instr: Instruction) -> String {
        let mnem = match instr.opcode {
            Opcode::Nop => "NOP",
            Opcode::Add => "ADD",
            Opcode::Sub => "SUB",
            Opcode::Mul => "MUL",
            Opcode::Div => "DIV",
            Opcode::Neg => "NEG",
            Opcode::Load => "LOAD",
            Opcode::Store => "STORE",
            Opcode::LoadIndex => "LOAD_INDEX",
            Opcode::StoreIndex => "STORE_INDEX",
            Opcode::Jmp => "JMP",
            Opcode::JmpN => {
                if instr.reg_idx == 1 {
                    "JMPGE"
                } else {
                    "JMPN"
                }
            }
            Opcode::JmpZ => {
                if instr.reg_idx == 1 {
                    "JMPNE"
                } else {
                    "JMPZ"
                }
            }
            Opcode::JmpP => {
                if instr.reg_idx == 1 {
                    "JMPLE"
                } else {
                    "JMPP"
                }
            }
            Opcode::Bceg => "BCEG",
            Opcode::Push => "PUSH",
            Opcode::Pop => "POP",
            Opcode::Call => "CALL",
            Opcode::Ret => "RET",
            Opcode::LoadImm => "LOAD_IMM",
            Opcode::AddImm => "ADD_IMM",
            Opcode::LoadIxImm => "LOAD_IX_IMM",
            Opcode::AddIxImm => "ADD_IX_IMM",
            Opcode::MovAccToIx => "MOV_ACC_IX",
            Opcode::MovIxToAcc => "MOV_IX_ACC",
            Opcode::Shift => {
                if instr.reg_idx > 0 {
                    "SHF_IX_ACC"
                } else {
                    "SHIFT"
                }
            }
            Opcode::MulInt => "MUL_INT",
            Opcode::DivInt => "DIV_INT",
            Opcode::CmpT => "CMP.T",
            Opcode::MajvT => "MAJV.T",
            Opcode::MinvT => "MINV.T",
            Opcode::AddIx => "ADD_IX",
            Opcode::DecIxJmpGe => "BIX",
            Opcode::LoadR => "LOAD_R",
            Opcode::MacPos => "MAC_POS",
            Opcode::MacNeg => "MAC_NEG",
            Opcode::Smt => "SMT",
            Opcode::Sat => "SAT",
            Opcode::Sit => "SIT",
            Opcode::NormT => "NORM.T",
            Opcode::Halt => "HALT",
        };

        match instr.opcode {
            Opcode::Nop
            | Opcode::Neg
            | Opcode::Push
            | Opcode::Pop
            | Opcode::Ret
            | Opcode::Halt => mnem.to_string(),
            Opcode::NormT => format!("{} IX{}", mnem, instr.reg_idx),
            Opcode::LoadIndex
            | Opcode::StoreIndex
            | Opcode::LoadIxImm
            | Opcode::AddIxImm
            | Opcode::AddIx
            | Opcode::DecIxJmpGe
            | Opcode::MajvT
            | Opcode::MinvT => {
                format!("{} IX{}, {}", mnem, instr.reg_idx, instr.operand_as_address())
            }
            Opcode::Shift if instr.reg_idx > 0 => {
                format!("{} IX{}, {}", mnem, instr.reg_idx, instr.operand)
            }
            Opcode::MovAccToIx | Opcode::MovIxToAcc => {
                format!("{} IX{}", mnem, instr.reg_idx)
            }
            Opcode::Bceg => {
                let (eq, gt) = Instruction::unpack_bceg_offsets(instr.operand);
                format!("{} IX{}, {:+}, {:+}", mnem, instr.reg_idx, eq, gt)
            }
            _ => format!("{} {}", mnem, instr.operand),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;
    use crate::memory::Memory;

    #[test]
    fn test_assembler_with_labels_and_ternary_literals() {
        let src = r#"
            ; Soma de contagem regressiva em Setun-ASM usando salto tricotomo JMPP
            LOAD_IX_IMM IX0, 5      ; IX0 = 5
            LOAD_IMM 0t0            ; ACC = 0
            STORE 300               ; inicializa soma parcial MEM[300] = 0
        .loop:
            MOV_IX_ACC IX0          ; ACC = IX0
            ADD 300                 ; ACC = IX0 + MEM[300]
            STORE 300               ; atualiza soma parcial em MEM[300]
            ADD_IX_IMM IX0, -1      ; IX0 -= 1
            MOV_IX_ACC IX0          ; ACC = IX0 para inspecionar sinal
            JMPP .loop              ; se IX0 > 0, repete
            HALT
        "#;

        let program = SetunAssembler::assemble(src).expect("Falha na montagem");
        let mut mem = Memory::new();
        let mut cpu = Cpu::new();
        cpu.load_program(&mut mem, 0, &program);
        cpu.run_ticks(&mut mem, 200);

        assert!(cpu.halted);
        // 5 + 4 + 3 + 2 + 1 = 15
        assert_eq!(mem.read_word(300).unwrap().as_i64(), 15);
    }
}
