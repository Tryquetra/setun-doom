//! Instruction selector and Setun-Doom code generator from LLVM IR functions.

use crate::codegen::ir::{IcmpCond, LlvmFunction, LlvmInst, LlvmValue};
use crate::cpu::{Instruction, Opcode};
use std::collections::{HashMap, HashSet};

/// Base memory address allocated for local stack frames / alloca objects (fits in 14-trit immediate space).
pub const CODEGEN_STACK_FRAME_BASE: i64 = 300_000;

/// Default buffer words allocated for local alloca buffers.
pub const ALLOCA_DEFAULT_BUFFER_SIZE: usize = 64;

/// Index register assigned as the Frame Pointer (FP).
pub const FP_REG: usize = 7;

/// Index register assigned as the pointer dereference scratchpad (PTR).
pub const PTR_REG: usize = 6;

/// Instruction selector for the Setun-Doom 21-trit architecture.
pub struct SetunCodeGenerator;

impl SetunCodeGenerator {
    /// Compiles an LLVM IR function into a native sequence of Setun [`Instruction`]s (entry point mode).
    pub fn compile_function(func: &LlvmFunction) -> Result<Vec<Instruction>, String> {
        let dummy_globals = HashMap::new();
        Self::compile_function_with_globals(func, true, &dummy_globals).map(|(insts, _)| insts)
    }

    /// Compiles an LLVM IR function, recording cross-function call site relocations.
    pub fn compile_function_with_relocs(
        func: &LlvmFunction,
        is_entry: bool,
    ) -> Result<(Vec<Instruction>, Vec<(usize, String)>), String> {
        let dummy_globals = HashMap::new();
        Self::compile_function_with_globals(func, is_entry, &dummy_globals)
    }

    /// Compiles an LLVM IR function with resolved global variables in linear memory.
    pub fn compile_function_with_globals(
        func: &LlvmFunction,
        is_entry: bool,
        global_map: &HashMap<String, usize>,
    ) -> Result<(Vec<Instruction>, Vec<(usize, String)>), String> {
        let mut instructions = Vec::new();
        let mut call_relocs: Vec<(usize, String)> = Vec::new();
        let mut cond_map: HashMap<String, IcmpCond> = HashMap::new();

        // 1. Allocate frame slots:
        //    - Parameters func.params
        //    - Local variables and SSA results
        //    - Alloca buffers
        let mut var_slots: HashMap<String, usize> = HashMap::new();
        let mut alloca_slots: HashMap<String, (usize, usize)> = HashMap::new();
        let mut next_slot = 0usize;

        // Allocate slots for incoming function parameters
        for param in &func.params {
            var_slots.insert(param.clone(), next_slot);
            next_slot += 1;
        }

        // Allocate slots for local SSA variables and alloca buffers
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    LlvmInst::Alloca { dst } => {
                        let ptr_slot = next_slot;
                        next_slot += 1;
                        let buf_slot = next_slot;
                        next_slot += ALLOCA_DEFAULT_BUFFER_SIZE;
                        var_slots.insert(dst.clone(), ptr_slot);
                        alloca_slots.insert(dst.clone(), (ptr_slot, buf_slot));
                    }
                    LlvmInst::Load { dst, .. }
                    | LlvmInst::GetElementPtr { dst, .. }
                    | LlvmInst::Add { dst, .. }
                    | LlvmInst::Sub { dst, .. }
                    | LlvmInst::Mul { dst, .. }
                    | LlvmInst::Sdiv { dst, .. }
                    | LlvmInst::Srem { dst, .. }
                    | LlvmInst::And { dst, .. }
                    | LlvmInst::Or { dst, .. }
                    | LlvmInst::Xor { dst, .. }
                    | LlvmInst::Shl { dst, .. }
                    | LlvmInst::Ashr { dst, .. }
                    | LlvmInst::Icmp { dst, .. }
                    | LlvmInst::Select { dst, .. }
                    | LlvmInst::Copy { dst, .. }
                    | LlvmInst::Phi { dst, .. } => {
                        var_slots.entry(dst.clone()).or_insert_with(|| {
                            let s = next_slot;
                            next_slot += 1;
                            s
                        });
                    }
                    LlvmInst::Call { dst, .. } => {
                        if let Some(d) = dst {
                            var_slots.entry(d.clone()).or_insert_with(|| {
                                let s = next_slot;
                                next_slot += 1;
                                s
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        let frame_size = next_slot.max(1);

        // 2. Pre-process PHI nodes: convert them into edge-specific copy operations
        let all_block_labels: HashSet<&str> =
            func.blocks.iter().map(|b| b.label.as_str()).collect();
        let entry_label = func
            .blocks
            .first()
            .map(|b| b.label.as_str())
            .unwrap_or("entry");
        let mut phi_edge_copies: HashMap<(String, String), Vec<(String, LlvmValue)>> =
            HashMap::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                if let LlvmInst::Phi { dst, incomings } = inst {
                    for (val, pred_lbl) in incomings {
                        let effective_pred = if all_block_labels.contains(pred_lbl.as_str()) {
                            pred_lbl.as_str()
                        } else {
                            entry_label
                        };
                        phi_edge_copies
                            .entry((effective_pred.to_string(), block.label.clone()))
                            .or_default()
                            .push((dst.clone(), val.clone()));
                    }
                }
            }
        }

        // 3. Emit Prologue
        if is_entry {
            // Set up Frame Pointer (IX7) at stack frame base
            instructions.push(Instruction::new(
                Opcode::LoadIxImm,
                FP_REG,
                CODEGEN_STACK_FRAME_BASE,
            ));
        }

        // Copy incoming arguments (in IX0..IX5) into parameter slots on this frame
        for (i, param) in func.params.iter().enumerate() {
            if i < PTR_REG {
                let slot = *var_slots.get(param).unwrap();
                instructions.push(Instruction::new(Opcode::MovIxToAcc, i, 0));
                instructions.push(Instruction::new(Opcode::StoreIndex, FP_REG, slot as i64));
            }
        }

        // Initialize alloca pointer variables to point to their buffer slots (IX7 + buf_slot)
        for (_alloca_var, (ptr_slot, buf_slot)) in &alloca_slots {
            instructions.push(Instruction::new(Opcode::MovIxToAcc, FP_REG, 0));
            instructions.push(Instruction::new(Opcode::AddImm, 0, *buf_slot as i64));
            instructions.push(Instruction::new(
                Opcode::StoreIndex,
                FP_REG,
                *ptr_slot as i64,
            ));
        }

        // 4. Translate basic blocks
        let mut block_labels: HashMap<String, usize> = HashMap::new();
        let mut pending_branches: Vec<(usize, String, Opcode, usize)> = Vec::new();

        for block in &func.blocks {
            block_labels.insert(block.label.clone(), instructions.len());

            for inst in &block.instructions {
                match inst {
                    LlvmInst::Alloca { .. } | LlvmInst::Phi { .. } => {
                        // Handled during slot allocation and block transitions
                    }

                    LlvmInst::Load { dst, ptr } => {
                        Self::load_val_into_acc(ptr, &var_slots, global_map, &mut instructions)?;
                        instructions.push(Instruction::new(Opcode::MovAccToIx, PTR_REG, 0));
                        instructions.push(Instruction::new(Opcode::LoadIndex, PTR_REG, 0));
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                    }

                    LlvmInst::Store { val, ptr } => {
                        // Set up pointer in PTR_REG
                        Self::load_val_into_acc(ptr, &var_slots, global_map, &mut instructions)?;
                        instructions.push(Instruction::new(Opcode::MovAccToIx, PTR_REG, 0));
                        // Load value into ACC and store through PTR_REG
                        Self::load_val_into_acc(val, &var_slots, global_map, &mut instructions)?;
                        instructions.push(Instruction::new(Opcode::StoreIndex, PTR_REG, 0));
                    }

                    LlvmInst::GetElementPtr { dst, base, offset } => {
                        Self::load_val_into_acc(base, &var_slots, global_map, &mut instructions)?;
                        match offset {
                            LlvmValue::ConstantInt(c) => {
                                if *c != 0 {
                                    instructions.push(Instruction::new(Opcode::AddImm, 0, *c));
                                }
                            }
                            _ => {
                                instructions.push(Instruction::new(Opcode::Store, 0, 790));
                                Self::load_val_into_acc(
                                    offset,
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::Add, 0, 790));
                            }
                        }
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                    }

                    LlvmInst::Add { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::Add,
                            dst,
                            lhs,
                            rhs,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Sub { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::Sub,
                            dst,
                            lhs,
                            rhs,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Mul { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::MulInt,
                            dst,
                            lhs,
                            rhs,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Sdiv { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::DivInt,
                            dst,
                            lhs,
                            rhs,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                    }

                    LlvmInst::Srem { dst, lhs, rhs } => {
                        // a % b = a - (a / b) * b
                        Self::load_val_into_acc(lhs, &var_slots, global_map, &mut instructions)?;
                        instructions.push(Instruction::new(Opcode::Store, 0, 792)); // lhs
                        Self::load_val_into_acc(rhs, &var_slots, global_map, &mut instructions)?;
                        instructions.push(Instruction::new(Opcode::Store, 0, 793)); // rhs

                        instructions.push(Instruction::new(Opcode::Load, 0, 792));
                        instructions.push(Instruction::new(Opcode::DivInt, 0, 793)); // ACC = lhs / rhs
                        instructions.push(Instruction::new(Opcode::MulInt, 0, 793)); // ACC = (lhs / rhs) * rhs
                        instructions.push(Instruction::new(Opcode::Store, 0, 794));

                        instructions.push(Instruction::new(Opcode::Load, 0, 792));
                        instructions.push(Instruction::new(Opcode::Sub, 0, 794)); // ACC = lhs - prod
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                    }

                    LlvmInst::And { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::And,
                            dst,
                            lhs,
                            rhs,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Or { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::Or,
                            dst,
                            lhs,
                            rhs,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                    }
                    LlvmInst::Xor { dst, lhs, rhs } => {
                        Self::emit_binary_op(
                            Opcode::Xor,
                            dst,
                            lhs,
                            rhs,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                    }

                    LlvmInst::Shl { dst, val, shift } => {
                        Self::load_val_into_acc(val, &var_slots, global_map, &mut instructions)?;
                        match shift {
                            LlvmValue::ConstantInt(c) => {
                                let multiplier = 1i64 << (*c as u32).min(31);
                                instructions.push(Instruction::new(Opcode::Store, 0, 790));
                                Self::emit_load_constant(multiplier, &mut instructions);
                                instructions.push(Instruction::new(Opcode::Store, 0, 791));
                                instructions.push(Instruction::new(Opcode::Load, 0, 790));
                                instructions.push(Instruction::new(Opcode::MulInt, 0, 791));
                            }
                            _ => {}
                        }
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                    }

                    LlvmInst::Ashr { dst, val, shift } => {
                        Self::load_val_into_acc(val, &var_slots, global_map, &mut instructions)?;
                        match shift {
                            LlvmValue::ConstantInt(c) => {
                                let divisor = 1i64 << (*c as u32).min(31);
                                instructions.push(Instruction::new(Opcode::Store, 0, 790));
                                Self::emit_load_constant(divisor, &mut instructions);
                                instructions.push(Instruction::new(Opcode::Store, 0, 791));
                                instructions.push(Instruction::new(Opcode::Load, 0, 790));
                                instructions.push(Instruction::new(Opcode::DivInt, 0, 791));
                            }
                            _ => {}
                        }
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                    }

                    LlvmInst::Copy { dst, src } => {
                        Self::load_val_into_acc(src, &var_slots, global_map, &mut instructions)?;
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                    }

                    LlvmInst::Icmp {
                        cond,
                        dst,
                        lhs,
                        rhs,
                    } => {
                        Self::load_val_into_acc(lhs, &var_slots, global_map, &mut instructions)?;
                        instructions.push(Instruction::new(Opcode::Store, 0, 790));
                        Self::load_val_into_acc(rhs, &var_slots, global_map, &mut instructions)?;
                        instructions.push(Instruction::new(Opcode::Store, 0, 791));
                        instructions.push(Instruction::new(Opcode::Load, 0, 790));
                        instructions.push(Instruction::new(Opcode::CmpT, 0, 791));

                        cond_map.insert(dst.clone(), cond.clone());
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                    }

                    LlvmInst::Select {
                        dst,
                        cond,
                        val_true,
                        val_false,
                    } => {
                        Self::load_val_into_acc(cond, &var_slots, global_map, &mut instructions)?;

                        let (true_op, true_reg) = match cond {
                            LlvmValue::Variable(c_var) => match cond_map.get(c_var) {
                                Some(IcmpCond::Eq) => (Opcode::JmpZ, 0),
                                Some(IcmpCond::Ne) => (Opcode::JmpZ, 1),
                                Some(IcmpCond::Sgt) => (Opcode::JmpP, 0),
                                Some(IcmpCond::Sge) => (Opcode::JmpN, 1),
                                Some(IcmpCond::Slt) => (Opcode::JmpN, 0),
                                Some(IcmpCond::Sle) => (Opcode::JmpP, 1),
                                None => (Opcode::JmpP, 0),
                            },
                            _ => (Opcode::JmpP, 0),
                        };

                        let jmp_true_idx = instructions.len();
                        instructions.push(Instruction::new(true_op, true_reg, 0));

                        // False branch:
                        Self::load_val_into_acc(
                            val_false,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;
                        let jmp_done_idx = instructions.len();
                        instructions.push(Instruction::new(Opcode::Jmp, 0, 0));

                        // True branch:
                        let true_pc = instructions.len() as i64;
                        instructions[jmp_true_idx] = Instruction::new(true_op, true_reg, true_pc);
                        Self::load_val_into_acc(
                            val_true,
                            &var_slots,
                            global_map,
                            &mut instructions,
                        )?;
                        Self::store_acc_into_var(dst, &var_slots, &mut instructions)?;

                        let done_pc = instructions.len() as i64;
                        instructions[jmp_done_idx] = Instruction::new(Opcode::Jmp, 0, done_pc);
                    }

                    LlvmInst::BrUncond { target } => {
                        if let Some(copies) =
                            phi_edge_copies.get(&(block.label.clone(), target.clone()))
                        {
                            Self::emit_copies(copies, &var_slots, global_map, &mut instructions)?;
                        }
                        pending_branches.push((instructions.len(), target.clone(), Opcode::Jmp, 0));
                        instructions.push(Instruction::new(Opcode::Jmp, 0, 0));
                    }

                    LlvmInst::BrCond {
                        cond,
                        if_true,
                        if_false,
                    } => {
                        Self::load_val_into_acc(cond, &var_slots, global_map, &mut instructions)?;

                        let (true_op, true_reg) = match cond {
                            LlvmValue::Variable(c_var) => match cond_map.get(c_var) {
                                Some(IcmpCond::Eq) => (Opcode::JmpZ, 0),
                                Some(IcmpCond::Ne) => (Opcode::JmpZ, 1),
                                Some(IcmpCond::Sgt) => (Opcode::JmpP, 0),
                                Some(IcmpCond::Sge) => (Opcode::JmpN, 1),
                                Some(IcmpCond::Slt) => (Opcode::JmpN, 0),
                                Some(IcmpCond::Sle) => (Opcode::JmpP, 1),
                                None => (Opcode::JmpP, 0),
                            },
                            _ => (Opcode::JmpP, 0),
                        };

                        let true_copies =
                            phi_edge_copies.get(&(block.label.clone(), if_true.clone()));
                        let false_copies =
                            phi_edge_copies.get(&(block.label.clone(), if_false.clone()));

                        if true_copies.is_none() || true_copies.unwrap().is_empty() {
                            // True path has no edge copies: branch directly to if_true
                            pending_branches.push((
                                instructions.len(),
                                if_true.clone(),
                                true_op,
                                true_reg,
                            ));
                            instructions.push(Instruction::new(true_op, true_reg, 0));

                            // False path:
                            if let Some(copies) = false_copies {
                                Self::emit_copies(
                                    copies,
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                            }
                            pending_branches.push((
                                instructions.len(),
                                if_false.clone(),
                                Opcode::Jmp,
                                0,
                            ));
                            instructions.push(Instruction::new(Opcode::Jmp, 0, 0));
                        } else {
                            // True path has edge copies: branch to a trampoline
                            let true_branch_idx = instructions.len();
                            instructions.push(Instruction::new(true_op, true_reg, 0));

                            // False path:
                            if let Some(copies) = false_copies {
                                Self::emit_copies(
                                    copies,
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                            }
                            pending_branches.push((
                                instructions.len(),
                                if_false.clone(),
                                Opcode::Jmp,
                                0,
                            ));
                            instructions.push(Instruction::new(Opcode::Jmp, 0, 0));

                            // True trampoline:
                            let true_trampoline_pc = instructions.len() as i64;
                            instructions[true_branch_idx] =
                                Instruction::new(true_op, true_reg, true_trampoline_pc);
                            Self::emit_copies(
                                true_copies.unwrap(),
                                &var_slots,
                                global_map,
                                &mut instructions,
                            )?;
                            pending_branches.push((
                                instructions.len(),
                                if_true.clone(),
                                Opcode::Jmp,
                                0,
                            ));
                            instructions.push(Instruction::new(Opcode::Jmp, 0, 0));
                        }
                    }

                    LlvmInst::Ret { val } => {
                        if let Some(v) = val {
                            Self::load_val_into_acc(v, &var_slots, global_map, &mut instructions)?;
                        }
                        if is_entry {
                            instructions.push(Instruction::new(Opcode::Halt, 0, 0));
                        } else {
                            instructions.push(Instruction::new(Opcode::Ret, 0, 0));
                        }
                    }

                    LlvmInst::Call { dst, callee, args } => {
                        if callee.starts_with("llvm.lifetime") {
                            // No-op intrinsic
                        } else if callee.starts_with("llvm.memset") {
                            // Forward to minilibc memset(dest, c, n)
                            if args.len() >= 3 {
                                Self::load_val_into_acc(
                                    &args[0],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::MovAccToIx, 0, 0));
                                Self::load_val_into_acc(
                                    &args[1],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::MovAccToIx, 1, 0));
                                Self::load_val_into_acc(
                                    &args[2],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::MovAccToIx, 2, 0));

                                instructions.push(Instruction::new(
                                    Opcode::AddIxImm,
                                    FP_REG,
                                    frame_size as i64,
                                ));
                                let call_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::Call, 0, 0));
                                call_relocs.push((call_idx, "memset".to_string()));
                                instructions.push(Instruction::new(
                                    Opcode::AddIxImm,
                                    FP_REG,
                                    -(frame_size as i64),
                                ));
                            }
                        } else if callee.starts_with("llvm.memcpy") {
                            // Forward to minilibc memcpy(dest, src, n)
                            if args.len() >= 3 {
                                Self::load_val_into_acc(
                                    &args[0],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::MovAccToIx, 0, 0));
                                Self::load_val_into_acc(
                                    &args[1],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::MovAccToIx, 1, 0));
                                Self::load_val_into_acc(
                                    &args[2],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::MovAccToIx, 2, 0));

                                instructions.push(Instruction::new(
                                    Opcode::AddIxImm,
                                    FP_REG,
                                    frame_size as i64,
                                ));
                                let call_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::Call, 0, 0));
                                call_relocs.push((call_idx, "memcpy".to_string()));
                                instructions.push(Instruction::new(
                                    Opcode::AddIxImm,
                                    FP_REG,
                                    -(frame_size as i64),
                                ));
                            }
                        } else if callee.starts_with("llvm.abs") {
                            if !args.is_empty() {
                                Self::load_val_into_acc(
                                    &args[0],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                let jmp_pos_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::JmpP, 0, 0));
                                let jmp_zero_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::JmpZ, 0, 0));
                                instructions.push(Instruction::new(Opcode::Neg, 0, 0));
                                let done_pc = instructions.len() as i64;
                                instructions[jmp_pos_idx] =
                                    Instruction::new(Opcode::JmpP, 0, done_pc);
                                instructions[jmp_zero_idx] =
                                    Instruction::new(Opcode::JmpZ, 0, done_pc);
                                if let Some(d) = dst {
                                    Self::store_acc_into_var(d, &var_slots, &mut instructions)?;
                                }
                            }
                        } else if callee == "llvm.smin.i32" || callee == "llvm.smin" {
                            if args.len() >= 2 {
                                Self::load_val_into_acc(
                                    &args[0],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::Store, 0, 790));
                                Self::load_val_into_acc(
                                    &args[1],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::Store, 0, 791));
                                instructions.push(Instruction::new(Opcode::Load, 0, 790));
                                instructions.push(Instruction::new(Opcode::CmpT, 0, 791));

                                let jmp_rhs_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::JmpP, 0, 0));
                                instructions.push(Instruction::new(Opcode::Load, 0, 790));
                                let jmp_done_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::Jmp, 0, 0));
                                let rhs_pc = instructions.len() as i64;
                                instructions[jmp_rhs_idx] =
                                    Instruction::new(Opcode::JmpP, 0, rhs_pc);
                                instructions.push(Instruction::new(Opcode::Load, 0, 791));
                                let done_pc = instructions.len() as i64;
                                instructions[jmp_done_idx] =
                                    Instruction::new(Opcode::Jmp, 0, done_pc);

                                if let Some(d) = dst {
                                    Self::store_acc_into_var(d, &var_slots, &mut instructions)?;
                                }
                            }
                        } else if callee == "llvm.smax.i32" || callee == "llvm.smax" {
                            if args.len() >= 2 {
                                Self::load_val_into_acc(
                                    &args[0],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::Store, 0, 790));
                                Self::load_val_into_acc(
                                    &args[1],
                                    &var_slots,
                                    global_map,
                                    &mut instructions,
                                )?;
                                instructions.push(Instruction::new(Opcode::Store, 0, 791));
                                instructions.push(Instruction::new(Opcode::Load, 0, 790));
                                instructions.push(Instruction::new(Opcode::CmpT, 0, 791));

                                let jmp_rhs_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::JmpN, 0, 0));
                                instructions.push(Instruction::new(Opcode::Load, 0, 790));
                                let jmp_done_idx = instructions.len();
                                instructions.push(Instruction::new(Opcode::Jmp, 0, 0));
                                let rhs_pc = instructions.len() as i64;
                                instructions[jmp_rhs_idx] =
                                    Instruction::new(Opcode::JmpN, 0, rhs_pc);
                                instructions.push(Instruction::new(Opcode::Load, 0, 791));
                                let done_pc = instructions.len() as i64;
                                instructions[jmp_done_idx] =
                                    Instruction::new(Opcode::Jmp, 0, done_pc);

                                if let Some(d) = dst {
                                    Self::store_acc_into_var(d, &var_slots, &mut instructions)?;
                                }
                            }
                        } else {
                            // General function call
                            // 1. Set up arguments in IX0..IX5
                            for (arg_i, arg_val) in args.iter().enumerate() {
                                if arg_i < PTR_REG {
                                    Self::load_val_into_acc(
                                        arg_val,
                                        &var_slots,
                                        global_map,
                                        &mut instructions,
                                    )?;
                                    instructions.push(Instruction::new(
                                        Opcode::MovAccToIx,
                                        arg_i,
                                        0,
                                    ));
                                }
                            }

                            // 2. Advance Frame Pointer (IX7) by frame_size for child callee
                            instructions.push(Instruction::new(
                                Opcode::AddIxImm,
                                FP_REG,
                                frame_size as i64,
                            ));

                            // 3. Emit Call instruction (relocated by linker)
                            let call_site_idx = instructions.len();
                            instructions.push(Instruction::new(Opcode::Call, 0, 0));
                            call_relocs.push((call_site_idx, callee.clone()));

                            // 4. Restore Frame Pointer (IX7)
                            instructions.push(Instruction::new(
                                Opcode::AddIxImm,
                                FP_REG,
                                -(frame_size as i64),
                            ));

                            // 5. Store return ACC if expected
                            if let Some(d) = dst {
                                Self::store_acc_into_var(d, &var_slots, &mut instructions)?;
                            }
                        }
                    }
                }
            }
        }

        // 5. Resolve branch targets
        for (idx, target_lbl, op, reg) in pending_branches {
            let target_pc = *block_labels
                .get(&target_lbl)
                .ok_or_else(|| format!("Branch target label not found: '{}'", target_lbl))?
                as i64;
            instructions[idx] = Instruction::new(op, reg, target_pc);
        }

        Ok((instructions, call_relocs))
    }

    /// Emits instructions to load any signed 21-trit constant into ACC,
    /// decomposing constants outside the 14-trit immediate range into balanced base-2187 components.
    pub fn emit_load_constant(c: i64, instructions: &mut Vec<Instruction>) {
        if c >= -2_391_484 && c <= 2_391_484 {
            instructions.push(Instruction::new(Opcode::LoadImm, 0, c));
        } else {
            let mut q = c / 2187;
            let mut r = c % 2187;
            if r > 1093 {
                r -= 2187;
                q += 1;
            } else if r < -1093 {
                r += 2187;
                q -= 1;
            }
            instructions.push(Instruction::new(Opcode::LoadImm, 0, q));
            instructions.push(Instruction::new(Opcode::Store, 0, 790));
            instructions.push(Instruction::new(Opcode::LoadImm, 0, 2187));
            instructions.push(Instruction::new(Opcode::Store, 0, 791));
            instructions.push(Instruction::new(Opcode::Load, 0, 790));
            instructions.push(Instruction::new(Opcode::MulInt, 0, 791));
            if r != 0 {
                instructions.push(Instruction::new(Opcode::AddImm, 0, r));
            }
        }
    }

    fn load_val_into_acc(
        val: &LlvmValue,
        var_slots: &HashMap<String, usize>,
        global_map: &HashMap<String, usize>,
        instructions: &mut Vec<Instruction>,
    ) -> Result<(), String> {
        match val {
            LlvmValue::ConstantInt(c) => {
                Self::emit_load_constant(*c, instructions);
            }
            LlvmValue::Variable(v) => {
                let slot = *var_slots
                    .get(v)
                    .ok_or_else(|| format!("Variable not found: {}", v))?;
                instructions.push(Instruction::new(Opcode::LoadIndex, FP_REG, slot as i64));
            }
            LlvmValue::Global(g) => {
                let addr = *global_map
                    .get(g)
                    .ok_or_else(|| format!("Global not found: {}", g))?
                    as i64;
                Self::emit_load_constant(addr, instructions);
            }
            LlvmValue::Label(_) => {}
        }
        Ok(())
    }

    fn store_acc_into_var(
        dst: &str,
        var_slots: &HashMap<String, usize>,
        instructions: &mut Vec<Instruction>,
    ) -> Result<(), String> {
        let slot = *var_slots
            .get(dst)
            .ok_or_else(|| format!("Destination variable not found: {}", dst))?;
        instructions.push(Instruction::new(Opcode::StoreIndex, FP_REG, slot as i64));
        Ok(())
    }

    fn emit_binary_op(
        op: Opcode,
        dst: &str,
        lhs: &LlvmValue,
        rhs: &LlvmValue,
        var_slots: &HashMap<String, usize>,
        global_map: &HashMap<String, usize>,
        instructions: &mut Vec<Instruction>,
    ) -> Result<(), String> {
        Self::load_val_into_acc(lhs, var_slots, global_map, instructions)?;

        match rhs {
            LlvmValue::ConstantInt(c) => match op {
                Opcode::Add if *c >= -2_391_484 && *c <= 2_391_484 => {
                    instructions.push(Instruction::new(Opcode::AddImm, 0, *c))
                }
                Opcode::Sub if -c >= -2_391_484 && -c <= 2_391_484 => {
                    instructions.push(Instruction::new(Opcode::AddImm, 0, -c))
                }
                _ => {
                    instructions.push(Instruction::new(Opcode::Store, 0, 790));
                    Self::emit_load_constant(*c, instructions);
                    instructions.push(Instruction::new(Opcode::Store, 0, 791));
                    instructions.push(Instruction::new(Opcode::Load, 0, 790));
                    instructions.push(Instruction::new(op, 0, 791));
                }
            },
            _ => {
                instructions.push(Instruction::new(Opcode::Store, 0, 790));
                Self::load_val_into_acc(rhs, var_slots, global_map, instructions)?;
                instructions.push(Instruction::new(Opcode::Store, 0, 791));
                instructions.push(Instruction::new(Opcode::Load, 0, 790));
                instructions.push(Instruction::new(op, 0, 791));
            }
        }

        Self::store_acc_into_var(dst, var_slots, instructions)?;
        Ok(())
    }

    fn emit_copies(
        copies: &[(String, LlvmValue)],
        var_slots: &HashMap<String, usize>,
        global_map: &HashMap<String, usize>,
        instructions: &mut Vec<Instruction>,
    ) -> Result<(), String> {
        // Push all source values onto hardware stack first to avoid dependency overwrites
        for (_dst, src) in copies {
            Self::load_val_into_acc(src, var_slots, global_map, instructions)?;
            instructions.push(Instruction::new(Opcode::Push, 0, 0));
        }
        // Pop into destinations in reverse order
        for (dst, _src) in copies.iter().rev() {
            instructions.push(Instruction::new(Opcode::Pop, 0, 0));
            Self::store_acc_into_var(dst, var_slots, instructions)?;
        }
        Ok(())
    }
}
