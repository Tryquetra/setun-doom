//! REBEL-6 code generation from LLVM IR.
//!
//! The 21-trit backend in [`crate::codegen::isel`] emits `Instruction` values for the accumulator
//! machine. This backend targets the REBEL-6 core instead: it emits `.tas` assembly for the
//! 32-trit, 729-register machine, which [`crate::rebel6::asm`] assembles into an image.
//!
//! Values live in a stack frame and registers are scratch, so the generator needs no allocator.
//! The calling convention is `abi-v0` from `docs/abi.md`: arguments in `a0` to `a7`, return in
//! `a0`, `ra` and `sp` as usual. Multiply and the signed division pair are calls to the helper
//! routines in [`PRELUDE`], because the base binary instruction set has no multiply or divide.

use std::collections::HashMap;

use crate::codegen::ir::{IcmpCond, LlvmFunction, LlvmInst, LlvmModule, LlvmValue};

/// Bytes per stack slot.
const SLOT: i64 = 4;

/// Words reserved for one `alloca` buffer.
const ALLOCA_WORDS: i64 = 64;

/// Register that carries a value between the frame and an operation.
const TMP: &str = "t0";
/// Second scratch register.
const TMP2: &str = "t1";
/// Third scratch register.
const TMP3: &str = "t2";

/// Runtime helpers emitted with every module.
pub const PRELUDE: &str = r#"	.text
	.globl _start
_start:
	aipc.t x1, %pcrel(main)
	jalr.t x1, 0(x1)
	li.t a7, 0
	ecall

	.globl __mulsi3
__mulsi3:
	li.t t0, 0
	li.t t3, 32
__mulsi3_loop:
	andi t1, a1, 1
	beq.t t1, x0, __mulsi3_shift
	add t0, t0, a0
__mulsi3_shift:
	slli a0, a0, 1
	srli a1, a1, 1
	addi t3, t3, -1
	bne.t t3, x0, __mulsi3_loop
	add a0, t0, x0
	jalr.t x0, 0(x1)

	.globl __udivmod
__udivmod:
	li.t t0, 0
	li.t t2, 0
	li.t t6, 0
	li.t t3, 32
__udivmod_loop:
	srli t4, a0, 31
	slli t6, t6, 1
	srli t5, t2, 31
	or t6, t6, t5
	slli t2, t2, 1
	or t2, t2, t4
	slli a0, a0, 1
	bne.t t6, x0, __udivmod_sub
	sltu t5, t2, a1
	bne.t t5, x0, __udivmod_skip
__udivmod_sub:
	sub t2, t2, a1
	li.t t6, 0
	slli t0, t0, 1
	ori t0, t0, 1
	jal.t x0, __udivmod_next
__udivmod_skip:
	slli t0, t0, 1
__udivmod_next:
	addi t3, t3, -1
	bne.t t3, x0, __udivmod_loop
	add a0, t0, x0
	add a2, t2, x0
	jalr.t x0, 0(x1)

	.globl __divsi3
__divsi3:
	addi sp, sp, -16
	sw ra, 0(sp)
	sw s0, 4(sp)
	xor s0, a0, a1
	srai s0, s0, 31
	srai t0, a0, 31
	xor a0, a0, t0
	sub a0, a0, t0
	srai t1, a1, 31
	xor a1, a1, t1
	sub a1, a1, t1
	aipc.t x1, %pcrel(__udivmod)
	jalr.t x1, 0(x1)
	xor a0, a0, s0
	sub a0, a0, s0
	lw ra, 0(sp)
	lw s0, 4(sp)
	addi sp, sp, 16
	jalr.t x0, 0(x1)

	.globl __modsi3
__modsi3:
	addi sp, sp, -16
	sw ra, 0(sp)
	sw s0, 4(sp)
	srai s0, a0, 31
	srai t0, a0, 31
	xor a0, a0, t0
	sub a0, a0, t0
	srai t1, a1, 31
	xor a1, a1, t1
	sub a1, a1, t1
	aipc.t x1, %pcrel(__udivmod)
	jalr.t x1, 0(x1)
	xor a2, a2, s0
	sub a0, a2, s0
	lw ra, 0(sp)
	lw s0, 4(sp)
	addi sp, sp, 16
	jalr.t x0, 0(x1)
"#;

/// Phi nodes of a block, each with its destination and its incoming edges.
type PhiEdges = HashMap<String, Vec<(String, Vec<(LlvmValue, String)>)>>;

/// Appends one line to the assembly output.
fn emit(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

/// Returns the name of the value an instruction defines, when it defines one.
fn dst_of(inst: &LlvmInst) -> Option<&str> {
    match inst {
        LlvmInst::Add { dst, .. }
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
        | LlvmInst::Copy { dst, .. }
        | LlvmInst::Phi { dst, .. }
        | LlvmInst::Alloca { dst }
        | LlvmInst::Load { dst, .. }
        | LlvmInst::Select { dst, .. }
        | LlvmInst::GetElementPtr { dst, .. } => Some(dst),
        LlvmInst::Call { dst, .. } => dst.as_deref(),
        LlvmInst::Store { .. } | LlvmInst::BrUncond { .. } | LlvmInst::BrCond { .. } | LlvmInst::Ret { .. } => None,
    }
}

/// REBEL-6 code generator.
pub struct Rebel6CodeGenerator;

impl Rebel6CodeGenerator {
    /// Compiles every function of `module` plus the runtime helpers into `.tas` assembly.
    ///
    /// # Errors
    ///
    /// Returns a message naming the function and the instruction when the IR uses a value the
    /// backend cannot express, such as a call with more than eight arguments.
    pub fn compile_module(module: &LlvmModule, entry: &str) -> Result<String, String> {
        if !module.functions.contains_key(entry) {
            return Err(format!("entry function `{entry}` is missing"));
        }
        let mut out = String::new();
        out.push_str(PRELUDE);
        out.push('\n');
        Self::emit_globals(module, &mut out);
        let mut names: Vec<&String> = module.functions.keys().collect();
        names.sort();
        for name in names {
            Self::compile_function(&module.functions[name], &mut out)?;
        }
        Ok(out)
    }

    /// Emits the `.data` and `.rodata` sections of the module.
    fn emit_globals(module: &LlvmModule, out: &mut String) {
        let mut names: Vec<&String> = module.globals.keys().collect();
        names.sort();
        for name in names {
            let global = &module.globals[name];
            let section = if global.is_constant { ".rodata" } else { ".data" };
            emit(out, &format!("\t.section {section}"));
            emit(out, &format!("\t.globl {name}"));
            emit(out, &format!("{name}:"));
            if global.words.is_empty() {
                emit(out, "\t.word 0");
            }
            for word in &global.words {
                emit(out, &format!("\t.word {word}"));
            }
        }
        if !module.globals.is_empty() {
            out.push('\n');
        }
    }

    /// Emits one function.
    fn compile_function(func: &LlvmFunction, out: &mut String) -> Result<(), String> {
        let mut slots: HashMap<String, i64> = HashMap::new();
        let mut next = SLOT;
        for param in &func.params {
            slots.insert(param.clone(), next);
            next += SLOT;
        }
        let mut buffers: Vec<(String, i64)> = Vec::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                if let Some(dst) = dst_of(inst) {
                    if slots.contains_key(dst) {
                        continue;
                    }
                    slots.insert(dst.to_string(), next);
                    next += SLOT;
                    if matches!(inst, LlvmInst::Alloca { .. }) {
                        buffers.push((dst.to_string(), next));
                        next += ALLOCA_WORDS * SLOT;
                    }
                }
            }
        }
        let frame = (next + 15) / 16 * 16;
        let ra_offset = 0;

        emit(out, &format!("\t.globl {}", func.name));
        emit(out, &format!("{}:", func.name));
        emit(out, &format!("\taddi sp, sp, -{frame}"));
        emit(out, &format!("\tsw ra, {ra_offset}(sp)"));
        for (index, param) in func.params.iter().enumerate() {
            if index >= 8 {
                return Err(format!("{}: more than eight parameters", func.name));
            }
            let offset = slots[param];
            emit(out, &format!("\tsw a{index}, {offset}(sp)"));
        }
        for (name, offset) in &buffers {
            let slot = slots[name];
            emit(out, &format!("\taddi {TMP}, sp, {offset}"));
            emit(out, &format!("\tsw {TMP}, {slot}(sp)"));
        }

        let phis = Self::collect_phis(func);
        for block in &func.blocks {
            emit(out, &format!(".L{}_{}:", func.name, block.label));
            for inst in &block.instructions {
                Self::compile_inst(func, block, inst, &slots, &phis, frame, out)?;
            }
        }
        Ok(())
    }

    /// Returns, per block, its phi nodes with their incoming edges.
    ///
    /// clang names the implicit entry block `0` in phi edges while the parser names it `entry`,
    /// so the edges are normalised to the parsed label of the first block.
    fn collect_phis(func: &LlvmFunction) -> PhiEdges {
        let labels: Vec<&str> = func.blocks.iter().map(|block| block.label.as_str()).collect();
        let entry = labels.first().copied().unwrap_or("entry");
        let mut phis = HashMap::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                if let LlvmInst::Phi { dst, incomings } = inst {
                    let incomings = incomings
                        .iter()
                        .map(|(value, from)| {
                            let from = if labels.contains(&from.as_str()) {
                                from.clone()
                            } else if from == "0" {
                                entry.to_string()
                            } else {
                                from.clone()
                            };
                            (value.clone(), from)
                        })
                        .collect();
                    phis.entry(block.label.clone())
                        .or_insert_with(Vec::new)
                        .push((dst.clone(), incomings));
                }
            }
        }
        phis
    }

    /// Emits the copies that give a phi its value on the edge from `from` to `to`.
    fn emit_edge(
        from: &str,
        to: &str,
        slots: &HashMap<String, i64>,
        phis: &PhiEdges,
        out: &mut String,
    ) -> Result<(), String> {
        let Some(entries) = phis.get(to) else {
            return Ok(());
        };
        for (dst, incomings) in entries {
            let Some((value, _)) = incomings.iter().find(|(_, block)| block == from) else {
                return Err(format!("phi {dst} has no incoming edge from {from}"));
            };
            Self::load_value(value, TMP, slots, out)?;
            emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
        }
        Ok(())
    }

    /// Emits one instruction.
    fn compile_inst(
        func: &LlvmFunction,
        block: &crate::codegen::ir::LlvmBlock,
        inst: &LlvmInst,
        slots: &HashMap<String, i64>,
        phis: &PhiEdges,
        frame: i64,
        out: &mut String,
    ) -> Result<(), String> {
        match inst {
            LlvmInst::Add { dst, lhs, rhs }
            | LlvmInst::Sub { dst, lhs, rhs }
            | LlvmInst::And { dst, lhs, rhs }
            | LlvmInst::Or { dst, lhs, rhs }
            | LlvmInst::Xor { dst, lhs, rhs } => {
                let mnemonic = match inst {
                    LlvmInst::Add { .. } => "add",
                    LlvmInst::Sub { .. } => "sub",
                    LlvmInst::And { .. } => "and",
                    LlvmInst::Or { .. } => "or",
                    _ => "xor",
                };
                Self::load_value(lhs, TMP, slots, out)?;
                Self::load_value(rhs, TMP2, slots, out)?;
                emit(out, &format!("\t{mnemonic} {TMP}, {TMP}, {TMP2}"));
                emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
            }
            LlvmInst::Mul { dst, lhs, rhs } => {
                Self::emit_helper_call(lhs, rhs, "__mulsi3", slots, out)?;
                emit(out, &format!("\tsw a0, {}(sp)", slots[dst]));
            }
            LlvmInst::Sdiv { dst, lhs, rhs } => {
                Self::emit_helper_call(lhs, rhs, "__divsi3", slots, out)?;
                emit(out, &format!("\tsw a0, {}(sp)", slots[dst]));
            }
            LlvmInst::Srem { dst, lhs, rhs } => {
                Self::emit_helper_call(lhs, rhs, "__modsi3", slots, out)?;
                emit(out, &format!("\tsw a0, {}(sp)", slots[dst]));
            }
            LlvmInst::Shl { dst, val, shift } => {
                Self::load_value(val, TMP, slots, out)?;
                Self::load_value(shift, TMP2, slots, out)?;
                emit(out, &format!("\tsll {TMP}, {TMP}, {TMP2}"));
                emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
            }
            LlvmInst::Ashr { dst, val, shift } => {
                Self::load_value(val, TMP, slots, out)?;
                Self::load_value(shift, TMP2, slots, out)?;
                emit(out, &format!("\tsra {TMP}, {TMP}, {TMP2}"));
                emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
            }
            LlvmInst::Icmp {
                dst,
                cond,
                lhs,
                rhs,
            } => {
                Self::load_value(lhs, TMP, slots, out)?;
                Self::load_value(rhs, TMP2, slots, out)?;
                match cond {
                    IcmpCond::Eq => {
                        emit(out, &format!("\tsub {TMP3}, {TMP}, {TMP2}"));
                        emit(out, &format!("\tsltiu {TMP3}, {TMP3}, 1"));
                    }
                    IcmpCond::Ne => {
                        emit(out, &format!("\tsub {TMP3}, {TMP}, {TMP2}"));
                        emit(out, &format!("\tsltu {TMP3}, x0, {TMP3}"));
                    }
                    IcmpCond::Slt => {
                        emit(out, &format!("\tslt {TMP3}, {TMP}, {TMP2}"));
                    }
                    IcmpCond::Sgt => {
                        emit(out, &format!("\tslt {TMP3}, {TMP2}, {TMP}"));
                    }
                    IcmpCond::Sle => {
                        emit(out, &format!("\tslt {TMP3}, {TMP2}, {TMP}"));
                        emit(out, &format!("\txori {TMP3}, {TMP3}, 1"));
                    }
                    IcmpCond::Sge => {
                        emit(out, &format!("\tslt {TMP3}, {TMP}, {TMP2}"));
                        emit(out, &format!("\txori {TMP3}, {TMP3}, 1"));
                    }
                }
                emit(out, &format!("\tsw {TMP3}, {}(sp)", slots[dst]));
            }
            LlvmInst::BrUncond { target } => {
                Self::emit_edge(&block.label, target, slots, phis, out)?;
                emit(out, &format!("\tjal.t x0, .L{}_{}", func.name, target));
            }
            LlvmInst::BrCond {
                cond,
                if_true,
                if_false,
            } => {
                Self::load_value(cond, TMP, slots, out)?;
                let from = &block.label;
                emit(out, &format!("\tbne.t {TMP}, x0, .L{}_edge_{}", func.name, if_true));
                Self::emit_edge(from, if_false, slots, phis, out)?;
                emit(out, &format!("\tjal.t x0, .L{}_{}", func.name, if_false));
                emit(out, &format!(".L{}_edge_{}:", func.name, if_true));
                Self::emit_edge(from, if_true, slots, phis, out)?;
                emit(out, &format!("\tjal.t x0, .L{}_{}", func.name, if_true));
            }
            LlvmInst::Ret { val } => {
                if let Some(value) = val {
                    Self::load_value(value, "a0", slots, out)?;
                }
                emit(out, "\tlw ra, 0(sp)");
                emit(out, &format!("\taddi sp, sp, {frame}"));
                emit(out, "\tjalr.t x0, 0(x1)");
            }
            LlvmInst::Copy { dst, src } => {
                Self::load_value(src, TMP, slots, out)?;
                emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
            }
            LlvmInst::Phi { .. } | LlvmInst::Alloca { .. } => {}
            LlvmInst::Load { dst, ptr } => {
                Self::load_value(ptr, TMP, slots, out)?;
                emit(out, &format!("\tlw {TMP2}, 0({TMP})"));
                emit(out, &format!("\tsw {TMP2}, {}(sp)", slots[dst]));
            }
            LlvmInst::Store { val, ptr } => {
                Self::load_value(val, TMP, slots, out)?;
                Self::load_value(ptr, TMP2, slots, out)?;
                emit(out, &format!("\tsw {TMP}, 0({TMP2})"));
            }
            LlvmInst::Select {
                dst,
                cond,
                val_true,
                val_false,
            } => {
                Self::load_value(cond, TMP, slots, out)?;
                emit(out, &format!("\tbne.t {TMP}, x0, .L{}_sel_{}", func.name, dst));
                Self::load_value(val_false, TMP, slots, out)?;
                emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
                emit(out, &format!("\tjal.t x0, .L{}_selend_{}", func.name, dst));
                emit(out, &format!(".L{}_sel_{}:", func.name, dst));
                Self::load_value(val_true, TMP, slots, out)?;
                emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
                emit(out, &format!(".L{}_selend_{}:", func.name, dst));
            }
            LlvmInst::Call { dst, callee, args } => {
                if args.len() > 8 {
                    return Err(format!("{callee}: more than eight arguments"));
                }
                for (index, arg) in args.iter().enumerate() {
                    Self::load_value(arg, &format!("a{index}"), slots, out)?;
                }
                emit(out, &format!("\taipc.t x1, %pcrel({callee})"));
                emit(out, "\tjalr.t x1, 0(x1)");
                if let Some(dst) = dst {
                    emit(out, &format!("\tsw a0, {}(sp)", slots[dst]));
                }
            }
            LlvmInst::GetElementPtr { dst, base, offset } => {
                Self::load_value(base, TMP, slots, out)?;
                Self::load_value(offset, TMP2, slots, out)?;
                emit(out, &format!("\tadd {TMP}, {TMP}, {TMP2}"));
                emit(out, &format!("\tsw {TMP}, {}(sp)", slots[dst]));
            }
        }
        Ok(())
    }

    /// Loads a value into `reg`.
    fn load_value(
        value: &LlvmValue,
        reg: &str,
        slots: &HashMap<String, i64>,
        out: &mut String,
    ) -> Result<(), String> {
        match value {
            LlvmValue::ConstantInt(constant) => {
                emit(out, &format!("\tli.t {reg}, {constant}"));
            }
            LlvmValue::Variable(name) => {
                let Some(offset) = slots.get(name) else {
                    return Err(format!("value {name} has no stack slot"));
                };
                emit(out, &format!("\tlw {reg}, {offset}(sp)"));
            }
            LlvmValue::Global(name) => {
                emit(out, &format!("\tli.t {reg}, {name}"));
            }
            LlvmValue::Label(name) => {
                return Err(format!("label {name} used as a value"));
            }
        }
        Ok(())
    }

    /// Emits a call to a two-argument helper.
    fn emit_helper_call(
        lhs: &LlvmValue,
        rhs: &LlvmValue,
        callee: &str,
        slots: &HashMap<String, i64>,
        out: &mut String,
    ) -> Result<(), String> {
        Self::load_value(lhs, "a0", slots, out)?;
        Self::load_value(rhs, "a1", slots, out)?;
        emit(out, &format!("\taipc.t x1, %pcrel({callee})"));
        emit(out, "\tjalr.t x1, 0(x1)");
        Ok(())
    }
}
