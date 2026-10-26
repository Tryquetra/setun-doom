//! Textual LLVM IR (`.ll`) parser emitted by Clang.

use crate::codegen::ir::{
    IcmpCond, LlvmBlock, LlvmFunction, LlvmGlobal, LlvmInst, LlvmModule, LlvmValue,
};

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
            } else if trimmed.starts_with('@') && trimmed.contains('=') {
                if let Some(global) = Self::parse_global(trimmed) {
                    module.globals.insert(global.name.clone(), global);
                }
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

        let mut depth = 0;
        let mut paren_end = None;
        for (idx, ch) in header[name_end..].char_indices() {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
                if depth == 0 {
                    paren_end = Some(name_end + idx);
                    break;
                }
            }
        }
        let paren_end = paren_end.ok_or("Argument list closing parenthesis not found")?;
        let params_str = &header[(name_end + 1)..paren_end];

        let mut param_chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut p_depth = 0;
        for ch in params_str.chars() {
            if ch == '(' {
                p_depth += 1;
            } else if ch == ')' {
                p_depth -= 1;
            } else if ch == ',' && p_depth == 0 {
                param_chunks.push(current_chunk.trim().to_string());
                current_chunk.clear();
                continue;
            }
            current_chunk.push(ch);
        }
        if !current_chunk.trim().is_empty() {
            param_chunks.push(current_chunk.trim().to_string());
        }

        let mut params = Vec::new();
        for param_chunk in param_chunks {
            for word in param_chunk.split_whitespace() {
                if word.starts_with('%') {
                    let p_name = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '%').to_string();
                    if !p_name.is_empty() {
                        params.push(p_name);
                        break;
                    }
                }
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
            let target = cleaned
                .strip_prefix("br label %")?
                .split_whitespace()
                .next()?
                .trim_matches(',')
                .to_string();
            return Some(LlvmInst::BrUncond { target });
        }

        // Conditional branch: br i1 %cond, label %true, label %false
        if cleaned.starts_with("br i1 ") {
            let remainder = cleaned.strip_prefix("br i1 ")?;
            let parts: Vec<&str> = remainder.split(',').collect();
            if parts.len() >= 3 {
                let cond = Self::parse_val(parts[0].trim());
                let if_true = parts[1]
                    .trim()
                    .strip_prefix("label %")?
                    .split_whitespace()
                    .next()?
                    .trim_matches(',')
                    .to_string();
                let if_false = parts[2]
                    .trim()
                    .strip_prefix("label %")?
                    .split_whitespace()
                    .next()?
                    .trim_matches(',')
                    .to_string();
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

        // Void call: call void @callee(...) or tail call void @callee(...)
        if cleaned.contains("call ") && !cleaned.contains('=') {
            return Self::parse_call(None, &cleaned);
        }

        // Assignments: %dst = <operation> ...
        if let Some((dst_part, rhs_part)) = cleaned.split_once('=') {
            let dst = dst_part.trim().to_string();
            let rhs = rhs_part.trim();

            if rhs.contains("call ") {
                return Self::parse_call(Some(dst), rhs);
            }

            let tokens: Vec<&str> = rhs.split_whitespace().collect();
            if tokens.is_empty() {
                return None;
            }

            let op = tokens[0];
            match op {
                "getelementptr" => {
                    let comma_parts: Vec<&str> = rhs.split(',').collect();
                    if comma_parts.len() >= 2 {
                        let mut base_tok = "";
                        for part in &comma_parts {
                            let words: Vec<&str> = part.split_whitespace().collect();
                            for (i, &w) in words.iter().enumerate() {
                                if (w == "ptr" || w.ends_with('*')) && i + 1 < words.len() {
                                    base_tok = words[i + 1];
                                    break;
                                }
                            }
                            if !base_tok.is_empty() {
                                break;
                            }
                        }
                        if base_tok.is_empty() && comma_parts.len() >= 2 {
                            base_tok = comma_parts[1].split_whitespace().last().unwrap_or("");
                        }
                        let last_part = comma_parts.last().unwrap_or(&"");
                        let offset_tok = last_part.split_whitespace().last().unwrap_or("0");

                        return Some(LlvmInst::GetElementPtr {
                            dst,
                            base: Self::parse_val(base_tok),
                            offset: Self::parse_val(offset_tok),
                        });
                    }
                    return None;
                }
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
                "add" | "sub" | "mul" | "sdiv" | "srem" | "and" | "or" | "xor" => {
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
                        "and" => Some(LlvmInst::And {
                            dst,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        }),
                        "or" => Some(LlvmInst::Or {
                            dst,
                            lhs: op_args.0,
                            rhs: op_args.1,
                        }),
                        "xor" => Some(LlvmInst::Xor {
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
                "sext" | "zext" | "trunc" | "inttoptr" | "ptrtoint" | "bitcast" => {
                    // Example: zext nneg i32 %3 to i64 or inttoptr i64 %6 to ptr
                    let to_idx = tokens.iter().position(|&t| t == "to")?;
                    if to_idx > 0 {
                        let src = Self::parse_val(tokens[to_idx - 1]);
                        Some(LlvmInst::Copy { dst, src })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn parse_call(dst: Option<String>, text: &str) -> Option<LlvmInst> {
        let callee_start = text.find('@')?;
        let open_paren = text[callee_start..].find('(')? + callee_start;
        let callee_raw = &text[callee_start + 1..open_paren].trim();
        let mut callee = callee_raw.trim_matches('"').to_string();

        if callee.starts_with("llvm.memcpy") {
            callee = "memcpy".to_string();
        } else if callee.starts_with("llvm.memset") {
            callee = "memset".to_string();
        } else if callee.starts_with("llvm.memmove") {
            callee = "memmove".to_string();
        }

        let close_paren = text.rfind(')')?;
        let args_str = text[open_paren + 1..close_paren].trim();

        let mut args = Vec::new();
        if !args_str.is_empty() {
            for arg_tok in args_str.split(',') {
                let trimmed = arg_tok.trim();
                if let Some(val_tok) = trimmed.split_whitespace().last() {
                    args.push(Self::parse_val(val_tok));
                }
            }
        }

        Some(LlvmInst::Call { dst, callee, args })
    }

    fn extract_binary_args(rhs: &str) -> Option<(LlvmValue, LlvmValue)> {
        let (lhs_str, rhs_str) = rhs.split_once(',')?;
        let lhs_tok = lhs_str.split_whitespace().last()?;
        let rhs_tok = rhs_str.split_whitespace().next()?;
        Some((Self::parse_val(lhs_tok), Self::parse_val(rhs_tok)))
    }

    fn parse_global(line: &str) -> Option<LlvmGlobal> {
        let (lhs, rhs) = line.split_once('=')?;
        let name = lhs.trim().trim_start_matches('@').trim_matches('"').to_string();
        let is_constant = rhs.contains(" constant ");

        let mut words = Vec::new();

        if let Some(str_start) = rhs.find("c\"") {
            if let Some(str_end) = rhs.rfind('"') {
                let content = &rhs[str_start + 2..str_end];
                let mut bytes = Vec::new();
                let mut chars = content.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '\\' {
                        let h1 = chars.next().unwrap_or('0');
                        let h2 = chars.next().unwrap_or('0');
                        let hex_s = format!("{}{}", h1, h2);
                        let b = u8::from_str_radix(&hex_s, 16).unwrap_or(0);
                        bytes.push(b as i64);
                    } else {
                        bytes.push(ch as i64);
                    }
                }
                words = bytes;
            }
        } else if rhs.contains("zeroinitializer") {
            let count = if let Some(bracket_idx) = rhs.find('[') {
                if let Some(x_idx) = rhs[bracket_idx..].find(" x ") {
                    let num_str = &rhs[bracket_idx + 1..bracket_idx + x_idx].trim();
                    num_str.parse::<usize>().unwrap_or(1)
                } else {
                    1
                }
            } else {
                1
            };
            words = vec![0; count];
        } else if let Some(arr_start) = rhs.find('[') {
            if let Some(arr_end) = rhs.rfind(']') {
                let inside = &rhs[arr_start + 1..arr_end];
                for elem in inside.split(',') {
                    if let Some(val_tok) = elem.trim().split_whitespace().last() {
                        let clean = val_tok.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                        if let Ok(v) = clean.parse::<i64>() {
                            words.push(v);
                        }
                    }
                }
            }
        } else {
            // Scalar integer literal: e.g. "global i32 42, align 4"
            let tokens: Vec<&str> = rhs.split_whitespace().collect();
            for (i, &tok) in tokens.iter().enumerate() {
                if (tok == "i32" || tok == "i64" || tok == "i16" || tok == "i8") && i + 1 < tokens.len() {
                    let val_tok = tokens[i + 1].trim_matches(',');
                    if let Ok(v) = val_tok.parse::<i64>() {
                        words.push(v);
                        break;
                    }
                }
            }
            if words.is_empty() {
                words.push(0);
            }
        }

        Some(LlvmGlobal {
            name,
            is_constant,
            words,
        })
    }

    fn parse_val(token: &str) -> LlvmValue {
        let clean = token.split(',').next().unwrap_or(token).trim();
        let clean = clean.trim_matches(|c: char| !c.is_alphanumeric() && c != '%' && c != '@' && c != '-');
        if clean.starts_with('%') {
            LlvmValue::Variable(clean.to_string())
        } else if clean.starts_with('@') {
            LlvmValue::Global(clean.trim_start_matches('@').to_string())
        } else if clean == "false" || clean == "null" {
            LlvmValue::ConstantInt(0)
        } else if clean == "true" {
            LlvmValue::ConstantInt(1)
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
