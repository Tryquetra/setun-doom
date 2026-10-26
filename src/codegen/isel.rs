//! Instruction selector and Setun-Doom code generator from LLVM IR functions.

use crate::codegen::ir::{LlvmFunction, LlvmInst, LlvmValue};
use crate::cpu::{Instruction, Opcode};
use std::collections::HashMap;

/// Base memory address allocated for local stack frames / alloca objects.
pub const CODEGEN_STACK_FRAME_BASE: i64 = 4_000_000;

/// Instruction selector for the Setun-Doom 21-trit architecture.
pub struct SetunCodeGenerator;

impl SetunCodeGenerator {
    /// Compiles an LLVM IR function into a native sequence of Setun [`Instruction`]s.
    pub fn compile_function(func: &LlvmFunction) -> Result<Vec<Instruction>, String> {
        let mut instructions = Vec::new();
        // Maps LLVM SSA variables to index registers IX0..IX8 or stack locations
        let mut var_map: HashMap<String, usize> = HashMap::new();
        // Maps alloca pointers to fixed addresses in linear memory
        let mut alloca_map: HashMap<String, i64> = HashMap::new();
        let mut next_alloca_addr = CODEGEN_STACK_FRAME_BASE;
        let mut next_reg = 0usize;

        // Label table for resolving branch targets to instruction offsets (PC)
        let mut block_labels: HashMap<String, usize> = HashMap::new();
        let mut pending_branches: Vec<(usize, String, Opcode, usize)> = Vec::new();

        // 1. Map function parameters to initial index registers (IX0, IX1, ...)
        for param in &func.params {
            if next_reg < 8 {
                var_map.insert(param.clone(), next_reg);
                next_reg += 1;
            } else {
                return Err("Too many parameters: architecture supports up to 8 index registers for direct ABI".to_string());
            }
        }

        // 2. Pre-process PHI nodes: eliminate PHIs by converting them into copy operations at predecessor tails
        let mut phi_copies: HashMap<String, Vec<(String, LlvmValue)>> = HashMap::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                if let LlvmInst::Phi { dst, incomings } = inst {
                    for (val, pred_lbl) in incomings {
                        phi_copies
                            .entry(pred_lbl.clone())
                            .or_default()
                            .push((dst.clone(), val.clone()));
                    }
                }
            }
        }

        // 3. Iterate through basic blocks and emit Setun instructions
        for block in &func.blocks {
            block_labels.insert(block.label.clone(), instructions.len());

            for inst in &block.instructions {
                match inst {
                    LlvmInst::Alloca { dst } => {
                        alloca_map.insert(dst.clone(), next_alloca_addr);
                        next_alloca_addr += 1;
                    }
                    LlvmInst::Store { val, ptr } => {
                        let ptr_addr = match ptr {
                            LlvmValue::Variable(var) => {
                                if let Some(&addr) = alloca_map.get(var) {
                                    addr
                                } else if let Some(&reg) = var_map.get(var) {
                                    // Pointer is dynamic inside an index register
                                    instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                                    0
                                } else {
                                    return Err(format!("Pointer variable not found: {}", var));
                                }
                            }
                            LlvmValue::ConstantInt(c) => *c,
                            _ => return Err("Invalid pointer for store".to_string()),
                        };

                        // Load value to ACC
                        match val {
                            LlvmValue::Variable(v) => {
                                let v_reg = *var_map.get(v).ok_or_else(|| format!("Value variable not found: {}", v))?;
                                instructions.push(Instruction::new(Opcode::MovIxToAcc, v_reg, 0));
                            }
                            LlvmValue::ConstantInt(c) => {
                                instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
                            }
                            _ => {}
                        }

                        if ptr_addr > 0 {
                            instructions.push(Instruction::new(Opcode::Store, 0, ptr_addr));
                        } else {
                            instructions.push(Instruction::new(Opcode::StoreIndex, 7, 0));
                        }
                    }
                    LlvmInst::Load { dst, ptr } => {
                        let ptr_addr = match ptr {
                            LlvmValue::Variable(var) => {
                                if let Some(&addr) = alloca_map.get(var) {
                                    addr
                                } else {
                                    return Err(format!("Pointer variable not found for load: {}", var));
                                }
                            }
                            LlvmValue::ConstantInt(c) => *c,
                            _ => return Err("Invalid pointer for load".to_string()),
                        };

                        instructions.push(Instruction::new(Opcode::Load, 0, ptr_addr));
                        let dst_reg = Self::get_or_alloc_reg(dst, &mut var_map, &mut next_reg)?;
                        instructions.push(Instruction::new(Opcode::MovAccToIx, dst_reg, 0));
                    }
                    LlvmInst::Add { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::Add,
                            dst,
                            lhs,
                            rhs,
                            &mut var_map,
                            &mut next_reg,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Sub { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::Sub,
                            dst,
                            lhs,
                            rhs,
                            &mut var_map,
                            &mut next_reg,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Mul { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::MulInt,
                            dst,
                            lhs,
                            rhs,
                            &mut var_map,
                            &mut next_reg,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Sdiv { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::DivInt,
                            dst,
                            lhs,
                            rhs,
                            &mut var_map,
                            &mut next_reg,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Copy { dst, src } => {
                        let dst_reg = Self::get_or_alloc_reg(dst, &mut var_map, &mut next_reg)?;
                        match src {
                            LlvmValue::Variable(var) => {
                                let src_reg = *var_map.get(var).ok_or_else(|| {
                                    format!("Source variable not found: {}", var)
                                })?;
                                instructions.push(Instruction::new(Opcode::MovIxToAcc, src_reg, 0));
                                instructions.push(Instruction::new(Opcode::MovAccToIx, dst_reg, 0));
                            }
                            LlvmValue::ConstantInt(c) => {
                                instructions.push(Instruction::new(Opcode::LoadIxImm, dst_reg, *c));
                            }
                            LlvmValue::Label(_) => {}
                        }
                    }
                    LlvmInst::Icmp { dst, cond: _, lhs, rhs } => {
                        // ACC = LHS <=> RHS using CMP.T
                        match lhs {
                            LlvmValue::Variable(var) => {
                                let reg = *var_map.get(var).ok_or_else(|| format!("LHS variable not found: {}", var))?;
                                instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                            }
                            LlvmValue::ConstantInt(c) => {
                                instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
                            }
                            _ => {}
                        }

                        // Compare against RHS
                        match rhs {
                            LlvmValue::Variable(var) => {
                                let reg = *var_map.get(var).ok_or_else(|| format!("RHS variable not found: {}", var))?;
                                instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0));
                                instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                                instructions.push(Instruction::new(Opcode::Store, 0, 799));
                                instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0));
                                instructions.push(Instruction::new(Opcode::CmpT, 0, 799));
                            }
                            LlvmValue::ConstantInt(c) => {
                                instructions.push(Instruction::new(Opcode::Store, 0, 799));
                                instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
                                instructions.push(Instruction::new(Opcode::Store, 0, 798));
                                instructions.push(Instruction::new(Opcode::Load, 0, 799));
                                instructions.push(Instruction::new(Opcode::CmpT, 0, 798));
                            }
                            _ => {}
                        }

                        let dst_reg = Self::get_or_alloc_reg(dst, &mut var_map, &mut next_reg)?;
                        instructions.push(Instruction::new(Opcode::MovAccToIx, dst_reg, 0));
                    }
                    LlvmInst::BrUncond { target } => {
                        // Apply PHI copies if any for this block transition
                        if let Some(copies) = phi_copies.get(&block.label) {
                            for (phi_dst, phi_val) in copies {
                                let dst_r = Self::get_or_alloc_reg(phi_dst, &mut var_map, &mut next_reg)?;
                                match phi_val {
                                    LlvmValue::Variable(v) => {
                                        if let Some(&sr) = var_map.get(v) {
                                            instructions.push(Instruction::new(Opcode::MovIxToAcc, sr, 0));
                                            instructions.push(Instruction::new(Opcode::MovAccToIx, dst_r, 0));
                                        }
                                    }
                                    LlvmValue::ConstantInt(c) => {
                                        instructions.push(Instruction::new(Opcode::LoadIxImm, dst_r, *c));
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let branch_idx = instructions.len();
                        instructions.push(Instruction::new(Opcode::Jmp, 0, 0));
                        pending_branches.push((branch_idx, target.clone(), Opcode::Jmp, 0));
                    }
                    LlvmInst::BrCond { cond, if_true, if_false } => {
                        // Load comparison condition into ACC
                        match cond {
                            LlvmValue::Variable(c_var) => {
                                if let Some(&r) = var_map.get(c_var) {
                                    instructions.push(Instruction::new(Opcode::MovIxToAcc, r, 0));
                                }
                            }
                            LlvmValue::ConstantInt(c) => {
                                instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
                            }
                            _ => {}
                        }

                        // JMPP jumps to if_true when CMP.T result > 0 (greater)
                        let branch_true_idx = instructions.len();
                        instructions.push(Instruction::new(Opcode::JmpP, 0, 0));
                        pending_branches.push((branch_true_idx, if_true.clone(), Opcode::JmpP, 0));

                        // Otherwise unconditionally jump to if_false
                        let branch_false_idx = instructions.len();
                        instructions.push(Instruction::new(Opcode::Jmp, 0, 0));
                        pending_branches.push((branch_false_idx, if_false.clone(), Opcode::Jmp, 0));
                    }
                    LlvmInst::Ret { val } => {
                        if let Some(ret_val) = val {
                            match ret_val {
                                LlvmValue::Variable(var) => {
                                    let reg = *var_map.get(var).ok_or_else(|| {
                                        format!("Return variable not found: {}", var)
                                    })?;
                                    // Move return value to Accumulator (ACC is the return register convention)
                                    instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                                }
                                LlvmValue::ConstantInt(c) => {
                                    instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
                                }
                                LlvmValue::Label(_) => {}
                            }
                        }
                        // In standalone functions without stack frame caller, HALT terminates execution cleanly
                        instructions.push(Instruction::new(Opcode::Halt, 0, 0));
                    }
                    _ => {}
                }
            }
        }

        // 4. Resolve pending branch target addresses
        for (idx, target_lbl, op, reg) in pending_branches {
            let target_pc = *block_labels.get(&target_lbl).ok_or_else(|| {
                format!("Branch target label not found: '{}'", target_lbl)
            })? as i64;
            instructions[idx] = Instruction::new(op, reg, target_pc);
        }

        Ok(instructions)
    }

    fn get_or_alloc_reg(
        var_name: &str,
        var_map: &mut HashMap<String, usize>,
        next_reg: &mut usize,
    ) -> Result<usize, String> {
        if let Some(&reg) = var_map.get(var_name) {
            Ok(reg)
        } else {
            // Allocate round-robin across registers IX0..IX6 (reserving IX7 as scratchpad)
            let reg = (*next_reg) % 7;
            *next_reg += 1;
            var_map.insert(var_name.to_string(), reg);
            Ok(reg)
        }
    }

    fn emit_binary_op(
        op: Opcode,
        dst: &str,
        lhs: &LlvmValue,
        rhs: &LlvmValue,
        var_map: &mut HashMap<String, usize>,
        next_reg: &mut usize,
        instructions: &mut Vec<Instruction>,
    ) -> Result<(), String> {
        // Load LHS into ACC
        match lhs {
            LlvmValue::Variable(var) => {
                let reg = *var_map
                    .get(var)
                    .ok_or_else(|| format!("Undeclared variable: {}", var))?;
                instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
            }
            LlvmValue::ConstantInt(c) => {
                instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
            }
            LlvmValue::Label(_) => {}
        }

        // Apply operation with RHS
        match rhs {
            LlvmValue::Variable(var) => {
                let reg = *var_map
                    .get(var)
                    .ok_or_else(|| format!("Undeclared variable: {}", var))?;
                match op {
                    Opcode::Add => {
                        // Store IX[reg] temporarily at scratch memory address 790, then ADD 790
                        instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0)); // preserve ACC
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                        instructions.push(Instruction::new(Opcode::Store, 0, 790));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0)); // restore ACC
                        instructions.push(Instruction::new(Opcode::Add, 0, 790));
                    }
                    Opcode::Sub => {
                        instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0)); // preserve ACC
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                        instructions.push(Instruction::new(Opcode::Store, 0, 790));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0)); // restore ACC
                        instructions.push(Instruction::new(Opcode::Sub, 0, 790));
                    }
                    Opcode::MulInt => {
                        instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                        instructions.push(Instruction::new(Opcode::Store, 0, 790));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0));
                        instructions.push(Instruction::new(Opcode::MulInt, 0, 790));
                    }
                    Opcode::DivInt => {
                        instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                        instructions.push(Instruction::new(Opcode::Store, 0, 790));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0));
                        instructions.push(Instruction::new(Opcode::DivInt, 0, 790));
                    }
                    _ => {
                        instructions.push(Instruction::new(op, reg, 0));
                    }
                }
            }
            LlvmValue::ConstantInt(c) => match op {
                Opcode::Add => {
                    instructions.push(Instruction::new(Opcode::AddImm, 0, *c));
                }
                Opcode::Sub => {
                    instructions.push(Instruction::new(Opcode::AddImm, 0, -c));
                }
                Opcode::MulInt => {
                    instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0));
                    instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
                    instructions.push(Instruction::new(Opcode::Store, 0, 790));
                    instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0));
                    instructions.push(Instruction::new(Opcode::MulInt, 0, 790));
                }
                Opcode::DivInt => {
                    instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0));
                    instructions.push(Instruction::new(Opcode::LoadImm, 0, *c));
                    instructions.push(Instruction::new(Opcode::Store, 0, 790));
                    instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0));
                    instructions.push(Instruction::new(Opcode::DivInt, 0, 790));
                }
                _ => {
                    instructions.push(Instruction::new(op, 0, *c));
                }
            },
            LlvmValue::Label(_) => {}
        }

        // Store result from ACC into destination register
        let dst_reg = Self::get_or_alloc_reg(dst, var_map, next_reg)?;
        instructions.push(Instruction::new(Opcode::MovAccToIx, dst_reg, 0));

        Ok(())
    }
}
