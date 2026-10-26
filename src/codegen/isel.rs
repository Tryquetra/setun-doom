//! Instruction selector and Setun-Doom code generator from LLVM IR functions.

use crate::codegen::ir::{LlvmFunction, LlvmInst, LlvmValue};
use crate::cpu::{Instruction, Opcode};
use std::collections::HashMap;

/// Instruction selector for the Setun-Doom 21-trit architecture.
pub struct SetunCodeGenerator;

impl SetunCodeGenerator {
    /// Compiles an LLVM IR function into a native sequence of Setun [`Instruction`]s.
    pub fn compile_function(func: &LlvmFunction) -> Result<Vec<Instruction>, String> {
        let mut instructions = Vec::new();
        // Maps LLVM SSA variables to index registers IX0..IX8 or stack locations
        let mut var_map: HashMap<String, usize> = HashMap::new();
        let mut next_reg = 0usize;

        // 1. Map function parameters to initial index registers (IX0, IX1, ...)
        for param in &func.params {
            if next_reg < 8 {
                var_map.insert(param.clone(), next_reg);
                next_reg += 1;
            } else {
                return Err("Too many parameters: architecture supports up to 8 index registers for direct ABI".to_string());
            }
        }

        // 2. Iterate through basic blocks and emit Setun instructions
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
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
                        instructions.push(Instruction::new(Opcode::Ret, 0, 0));
                    }
                    _ => {}
                }
            }
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
        } else if *next_reg < 8 {
            let reg = *next_reg;
            *next_reg += 1;
            var_map.insert(var_name.to_string(), reg);
            Ok(reg)
        } else {
            Err("Index registers exhausted (register allocator requires stack spilling)".to_string())
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
                        // ACC = ACC + IX[reg] via temporary index IX7
                        instructions.push(Instruction::new(Opcode::MovAccToIx, 7, 0));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, reg, 0));
                        instructions.push(Instruction::new(Opcode::AddIx, 7, 0));
                        instructions.push(Instruction::new(Opcode::MovIxToAcc, 7, 0));
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
