//! Textual LLVM IR (`.ll`) parser emitted by Clang.

use crate::codegen::ir::{IcmpCond, LlvmBlock, LlvmFunction, LlvmInst, LlvmModule, LlvmValue};

/// Parser for textual LLVM IR.
pub struct LlvmParser;

impl LlvmParser {
    /// Parses a string containing textual LLVM IR (`.ll`) into functions and basic blocks.
    pub fn parse(input: &str) -> Result<LlvmModule, String> {
        let mut module = LlvmModule::default();
        let mut lines = input.lines().peekable();

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if trimmed.starts_with("define ") {
                let func = Self::parse_function(trimmed, &mut lines)?;
                module.functions.insert(func.name.clone(), func);
            }
        }

        Ok(module)
    }

    fn parse_function<'a, I>(
        header: &str,
        lines: &mut I,
    ) -> Result<LlvmFunction, String>
    where
        I: Iterator<Item = &'a str>,
    {
        // Example: define dso_local i32 @fixed_mul(i32 noundef %0, i32 noundef %1) ... {
        let name_start = header.find('@').ok_or("Function name not found in header")?;
        let name_end = header[name_start..]
            .find('(')
            .ok_or("Argument list opening parenthesis not found")?
            + name_start;
        let func_name = header[(name_start + 1)..name_end].trim().to_string();

        let paren_end = header[name_end..]
            .find(')')
            .ok_or("Argument list closing parenthesis not found")?
            + name_end;
        let params_str = &header[(name_end + 1)..paren_end];

        let mut params = Vec::new();
        for param_chunk in params_str.split(',') {
            let p_trim = param_chunk.trim();
            if let Some(percent_idx) = p_trim.rfind('%') {
                let p_name = p_trim[percent_idx..]
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '%')
                    .to_string();
                params.push(p_name);
            }
        }

        let mut blocks = Vec::new();
        let mut current_block = LlvmBlock {
            label: "entry".to_string(),
            instructions: Vec::new(),
        };

        for line in lines {
            let t = line.trim();
            if t == "}" {
                if !current_block.instructions.is_empty() || blocks.is_empty() {
                    blocks.push(current_block);
                }
                break;
            }
            if t.is_empty() || t.starts_with(';') {
                continue;
            }

            // Checks if line is a block label (e.g. "return:" or "6:" or "10: ; preds = ...")
            let before_comment = t.split(';').next().unwrap_or("").trim();
            if let Some(lbl) = before_comment.strip_suffix(':') {
                if !current_block.instructions.is_empty() || current_block.label != "entry" {
                    blocks.push(current_block);
                }
                current_block = LlvmBlock {
                    label: lbl.trim().to_string(),
                    instructions: Vec::new(),
                };
                continue;
            }

            if let Some(inst) = Self::parse_instruction(t) {
                current_block.instructions.push(inst);
            }
        }

        Ok(LlvmFunction {
            name: func_name,
            params,
            blocks,
        })
    }

    fn parse_instruction(line: &str) -> Option<LlvmInst> {
        let cleaned = line.split(';').next()?.trim();
        if cleaned.is_empty() {
            return None;
        }

        // Return: ret i32 %val or ret void
        if cleaned.starts_with("ret ") {
            let parts: Vec<&str> = cleaned.split_whitespace().collect();
            if parts.len() >= 3 {
                let val = Self::parse_val(parts[2]);
                return Some(LlvmInst::Ret { val: Some(val) });
            } else {
                return Some(LlvmInst::Ret { val: None });
            }
        }

        // Unconditional branch: br label %lbl
        if cleaned.starts_with("br label %") {
            let target = cleaned.strip_prefix("br label %")?.trim().to_string();
            return Some(LlvmInst::BrUncond { target });
        }

        // Conditional branch: br i1 %cond, label %true, label %false
        if cleaned.starts_with("br i1 ") {
            let remainder = cleaned.strip_prefix("br i1 ")?;
            let parts: Vec<&str> = remainder.split(',').collect();
            if parts.len() == 3 {
                let cond = Self::parse_val(parts[0].trim());
                let if_true = parts[1].trim().strip_prefix("label %")?.trim().to_string();
                let if_false = parts[2].trim().strip_prefix("label %")?.trim().to_string();
                return Some(LlvmInst::BrCond {
                    cond,
                    if_true,
                    if_false,
                });
            }
        }

        // Store: store i32 %val, ptr %ptr, align 4 or store volatile i32 %val, ptr %ptr, align 4
        if cleaned.starts_with("store ") {
            let tokens: Vec<&str> = cleaned.split_whitespace().collect();
            let mut val_tok = "";
            let mut ptr_tok = "";
            for (i, tok) in tokens.iter().enumerate() {
                if (*tok == "ptr" || tok.ends_with('*')) && i + 1 < tokens.len() {
                    ptr_tok = tokens[i + 1];
                    if i >= 1 {
                        val_tok = tokens[i - 1];
                    }
                    break;
                }
            }
            if !val_tok.is_empty() && !ptr_tok.is_empty() {
                return Some(LlvmInst::Store {
                    val: Self::parse_val(val_tok),
                    ptr: Self::parse_val(ptr_tok),
                });
            }
        }

        // Assignments: %dst = <operation> ...
        if let Some((dst_part, rhs_part)) = cleaned.split_once('=') {
            let dst = dst_part.trim().to_string();
            let rhs = rhs_part.trim();

            let tokens: Vec<&str> = rhs.split_whitespace().collect();
            if tokens.is_empty() {
                return None;
            }

            let op = tokens[0];
            match op {
                "alloca" => {
                    return Some(LlvmInst::Alloca { dst });
                }
                "load" => {
                    // Example: load i32, ptr %x, align 4 or load volatile i32, ptr %3, align 4
                    let mut ptr_tok = "";
                    for (i, tok) in tokens.iter().enumerate() {
                        if (*tok == "ptr" || tok.ends_with('*')) && i + 1 < tokens.len() {
                            ptr_tok = tokens[i + 1];
                            break;
                        }
                    }
                    if ptr_tok.is_empty() {
                        ptr_tok = tokens.last()?;
                    }
                    return Some(LlvmInst::Load {
                        dst,
                        ptr: Self::parse_val(ptr_tok),
                    });
                }
                "phi" => {
                    // Example: phi i32 [ %val1, %label1 ], [ %val2, %label2 ]
                    let mut incomings = Vec::new();
                    for bracket in rhs.split('[') {
                        if let Some(inside) = bracket.split(']').next() {
                            if let Some((val_s, lbl_s)) = inside.split_once(',') {
                                let v = Self::parse_val(val_s.trim());
                                let lbl = lbl_s
                                    .trim()
                                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '%')
                                    .trim_start_matches('%')
                                    .to_string();
                                if !lbl.is_empty() {
                                    incomings.push((v, lbl));
                                }
                            }
                        }
                    }
                    return Some(LlvmInst::Phi { dst, incomings });
                }
                "select" => {
                    // Example: select i1 %4, i32 %5, i32 %2
                    let parts: Vec<&str> = rhs.split(',').collect();
                    if parts.len() == 3 {
                        let cond_tok = parts[0].split_whitespace().last()?;
                        let true_tok = parts[1].split_whitespace().last()?;
                        let false_tok = parts[2].split_whitespace().last()?;
                        return Some(LlvmInst::Select {
                            dst,
                            cond: Self::parse_val(cond_tok),
                            val_true: Self::parse_val(true_tok),
                            val_false: Self::parse_val(false_tok),
                        });
                    }
                    return None;
                }
                "add" | "sub" | "mul" | "sdiv" | "srem" => {
                    // Example: add nsw i32 %a, %b or add i32 %a, 10
                    let op_args = Self::extract_binary_args(rhs)?;
                    match op {
                        "add" => Some(LlvmInst::Add {
                            dst,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        }),
                        "sub" => Some(LlvmInst::Sub {
                            dst,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        }),
                        "mul" => Some(LlvmInst::Mul {
                            dst,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        }),
                        "sdiv" => Some(LlvmInst::Sdiv {
                            dst,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        }),
                        "srem" => Some(LlvmInst::Srem {
                            dst,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        }),
                        _ => None,
                    }
                }
                "shl" | "ashr" | "lshr" => {
                    let op_args = Self::extract_binary_args(rhs)?;
                    match op {
                        "shl" => Some(LlvmInst::Shl {
                            dst,
                            val: op_args.0,
                            shift: op_args.1,
                        }),
                        _ => Some(LlvmInst::Ashr {
                            dst,
                            val: op_args.0,
                            shift: op_args.1,
                        }),
                    }
                }
                "icmp" => {
                    // Example: icmp slt i32 %a, %b
                    if tokens.len() >= 4 {
                        let cond = IcmpCond::from_str(tokens[1])?;
                        let op_args = Self::extract_binary_args(rhs)?;
                        Some(LlvmInst::Icmp {
                            dst,
                            cond,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        })
                    } else {
                        None
                    }
                }
                "sext" | "zext" | "trunc" => {
                    // Example: sext i32 %0 to i64
                    let last_token = tokens.last()?;
                    let src = Self::parse_val(tokens.get(2).unwrap_or(last_token));
                    Some(LlvmInst::Copy { dst, src })
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn extract_binary_args(rhs: &str) -> Option<(LlvmValue, LlvmValue)> {
        let (lhs_str, rhs_str) = rhs.split_once(',')?;
        let lhs_tok = lhs_str.split_whitespace().last()?;
        let rhs_tok = rhs_str.split_whitespace().next()?;
        Some((Self::parse_val(lhs_tok), Self::parse_val(rhs_tok)))
    }

    fn parse_val(token: &str) -> LlvmValue {
        let clean = token.split(',').next().unwrap_or(token).trim();
        let clean = clean.trim_matches(|c: char| !c.is_alphanumeric() && c != '%' && c != '-');
        if clean.starts_with('%') {
            LlvmValue::Variable(clean.to_string())
        } else if let Ok(val) = clean.parse::<i64>() {
            LlvmValue::ConstantInt(val)
        } else {
            LlvmValue::Variable(clean.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_fixed_mul_function() {
        let ir = r#"
        define dso_local i32 @fixed_mul_sample(i32 noundef %a, i32 noundef %b) {
          %1 = mul nsw i32 %a, %b
          %2 = ashr i32 %1, 16
          ret i32 %2
        }
        "#;

        let module = LlvmParser::parse(ir).expect("LLVM parser error");
        assert!(module.functions.contains_key("fixed_mul_sample"));
        let func = &module.functions["fixed_mul_sample"];
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.blocks.len(), 1);
        let insts = &func.blocks[0].instructions;
        assert_eq!(insts.len(), 3);
    }
}
