// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Two-stack ternary machine inspired by the **Setun-70** (1970).
//!
//! The historical Setun-70 replaced the accumulator with two hardware stacks: an operand stack
//! evaluated in reverse Polish notation and a return stack for subroutines. Instructions are read
//! as a continuous stream of trytes, with the opcode in one tryte and operands in two trytes.
//!
//! This module is an independent execution model. It does not touch the accumulator CPU or the
//! DBT, so the playable path keeps its performance.

use core::fmt;

use crate::memory::{Memory, MemoryError};
use crate::word::{wrap_balanced, Word21};

/// Operand stack top, descending toward lower addresses.
pub const OPERAND_STACK_TOP: i64 = 900_000;
/// Lowest operand stack address (inclusive).
pub const OPERAND_STACK_LIMIT: i64 = 890_000;
/// Return stack top, descending toward lower addresses.
pub const RETURN_STACK_TOP: i64 = 880_000;
/// Lowest return stack address (inclusive).
pub const RETURN_STACK_LIMIT: i64 = 870_000;

/// Trytes per operand in the instruction stream.
pub const OPERAND_TRYTES: usize = 2;

// The two stacks must occupy disjoint address ranges.
const _: () = assert!(OPERAND_STACK_LIMIT > RETURN_STACK_TOP);

/// Opcodes of the two-stack machine.
#[repr(i64)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StackOp {
    /// Stops the machine.
    Halt = 0,
    /// Pushes the following two-tryte operand.
    Push = 1,
    /// Discards the top of the operand stack.
    Pop = 2,
    /// Duplicates the top of the operand stack.
    Dup = 3,
    /// Swaps the two top elements.
    Swap = 4,
    /// Adds the two top elements.
    Add = 5,
    /// Subtracts the top from the one below it.
    Sub = 6,
    /// Multiplies the two top elements.
    Mul = 7,
    /// Divides the element below the top by the top, truncating toward zero.
    Div = 8,
    /// Unconditional jump to the following two-tryte address.
    Jmp = 9,
    /// Pops a value and jumps when it is zero.
    Jz = 10,
    /// Pushes the next address and jumps to the following two-tryte address.
    Call = 11,
    /// Returns to the address on the return stack.
    Ret = 12,
    /// Pops an address and pushes the memory word stored there.
    Load = 13,
    /// Pops an address and a value and stores the value at that address.
    Store = 14,
}

impl StackOp {
    /// Decodes a tryte into an opcode.
    #[must_use]
    pub const fn from_tryte(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Halt),
            1 => Some(Self::Push),
            2 => Some(Self::Pop),
            3 => Some(Self::Dup),
            4 => Some(Self::Swap),
            5 => Some(Self::Add),
            6 => Some(Self::Sub),
            7 => Some(Self::Mul),
            8 => Some(Self::Div),
            9 => Some(Self::Jmp),
            10 => Some(Self::Jz),
            11 => Some(Self::Call),
            12 => Some(Self::Ret),
            13 => Some(Self::Load),
            14 => Some(Self::Store),
            _ => None,
        }
    }
}

/// Errors raised by the two-stack machine.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StackError {
    /// Access fault in ternary memory.
    Memory(MemoryError),
    /// Operand stack underflow.
    OperandUnderflow,
    /// Return stack underflow.
    ReturnUnderflow,
    /// Operand stack overflow.
    OperandOverflow,
    /// Return stack overflow.
    ReturnOverflow,
    /// Program counter outside the tryte stream.
    ProgramCounterOutOfBounds(usize),
    /// Undefined opcode tryte.
    InvalidOpcode(i64),
    /// Division by zero.
    DivisionByZero,
    /// Step limit exceeded.
    StepLimitExceeded(u64),
}

impl fmt::Display for StackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory(error) => write!(f, "memory fault: {error}"),
            Self::OperandUnderflow => write!(f, "operand stack underflow"),
            Self::ReturnUnderflow => write!(f, "return stack underflow"),
            Self::OperandOverflow => write!(f, "operand stack overflow"),
            Self::ReturnOverflow => write!(f, "return stack overflow"),
            Self::ProgramCounterOutOfBounds(pc) => {
                write!(f, "program counter {pc} outside the tryte stream")
            }
            Self::InvalidOpcode(value) => write!(f, "invalid opcode tryte {value}"),
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::StepLimitExceeded(limit) => write!(f, "step limit exceeded: {limit}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StackError {}

impl From<MemoryError> for StackError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}

/// State of the two-stack machine.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StackMachine {
    /// Program counter as a tryte index.
    pub pc: usize,
    /// Operand stack pointer, pointing at the next free cell below the top.
    pub operand_sp: i64,
    /// Return stack pointer, pointing at the next free cell below the top.
    pub return_sp: i64,
    /// Machine halted.
    pub halted: bool,
    /// Instructions executed.
    pub steps: u64,
    /// Operand stack cells read or written.
    pub words_moved: u64,
}

impl Default for StackMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StackMachine {
    /// Creates a machine with both stacks empty.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pc: 0,
            operand_sp: OPERAND_STACK_TOP,
            return_sp: RETURN_STACK_TOP,
            halted: false,
            steps: 0,
            words_moved: 0,
        }
    }

    /// Pushes `value` onto the operand stack.
    pub fn push(&mut self, memory: &mut Memory, value: i64) -> Result<(), StackError> {
        if self.operand_sp < OPERAND_STACK_LIMIT {
            return Err(StackError::OperandOverflow);
        }
        memory.write_word(self.operand_sp, Word21::new(value))?;
        self.operand_sp -= 1;
        self.words_moved += 1;
        Ok(())
    }

    /// Pops the top of the operand stack.
    pub fn pop(&mut self, memory: &Memory) -> Result<i64, StackError> {
        if self.operand_sp >= OPERAND_STACK_TOP {
            return Err(StackError::OperandUnderflow);
        }
        self.operand_sp += 1;
        self.words_moved += 1;
        Ok(memory.read_word(self.operand_sp)?.as_i64())
    }

    /// Pushes `value` onto the return stack.
    fn push_return(&mut self, memory: &mut Memory, value: i64) -> Result<(), StackError> {
        if self.return_sp < RETURN_STACK_LIMIT {
            return Err(StackError::ReturnOverflow);
        }
        memory.write_word(self.return_sp, Word21::new(value))?;
        self.return_sp -= 1;
        self.words_moved += 1;
        Ok(())
    }

    /// Pops the top of the return stack.
    fn pop_return(&mut self, memory: &Memory) -> Result<i64, StackError> {
        if self.return_sp >= RETURN_STACK_TOP {
            return Err(StackError::ReturnUnderflow);
        }
        self.return_sp += 1;
        self.words_moved += 1;
        Ok(memory.read_word(self.return_sp)?.as_i64())
    }

    /// Reads a two-tryte operand at `pc` and advances past it.
    fn operand(code: &[i64], pc: &mut usize) -> Result<i64, StackError> {
        if *pc + OPERAND_TRYTES > code.len() {
            return Err(StackError::ProgramCounterOutOfBounds(*pc));
        }
        let low = code[*pc];
        let high = code[*pc + 1];
        *pc += OPERAND_TRYTES;
        Ok(low + high * 729)
    }

    /// Executes one instruction from `code`.
    pub fn step(&mut self, memory: &mut Memory, code: &[i64]) -> Result<(), StackError> {
        if self.halted {
            return Ok(());
        }
        if self.pc >= code.len() {
            return Err(StackError::ProgramCounterOutOfBounds(self.pc));
        }

        let raw = code[self.pc];
        self.pc += 1;
        let opcode = StackOp::from_tryte(raw).ok_or(StackError::InvalidOpcode(raw))?;
        self.steps += 1;

        match opcode {
            StackOp::Halt => self.halted = true,
            StackOp::Push => {
                let value = Self::operand(code, &mut self.pc)?;
                self.push(memory, value)?;
            }
            StackOp::Pop => {
                self.pop(memory)?;
            }
            StackOp::Dup => {
                let value = self.pop(memory)?;
                self.push(memory, value)?;
                self.push(memory, value)?;
            }
            StackOp::Swap => {
                let top = self.pop(memory)?;
                let below = self.pop(memory)?;
                self.push(memory, top)?;
                self.push(memory, below)?;
            }
            StackOp::Add => {
                let rhs = self.pop(memory)?;
                let lhs = self.pop(memory)?;
                self.push(memory, lhs + rhs)?;
            }
            StackOp::Sub => {
                let rhs = self.pop(memory)?;
                let lhs = self.pop(memory)?;
                self.push(memory, lhs - rhs)?;
            }
            StackOp::Mul => {
                let rhs = self.pop(memory)?;
                let lhs = self.pop(memory)?;
                self.push(memory, lhs * rhs)?;
            }
            StackOp::Div => {
                let rhs = self.pop(memory)?;
                let lhs = self.pop(memory)?;
                if rhs == 0 {
                    return Err(StackError::DivisionByZero);
                }
                self.push(memory, lhs / rhs)?;
            }
            StackOp::Jmp => {
                let target = Self::operand(code, &mut self.pc)?;
                self.jump_to(target)?;
            }
            StackOp::Jz => {
                let target = Self::operand(code, &mut self.pc)?;
                let value = self.pop(memory)?;
                if value == 0 {
                    self.jump_to(target)?;
                }
            }
            StackOp::Call => {
                let target = Self::operand(code, &mut self.pc)?;
                self.push_return(memory, self.pc as i64)?;
                self.jump_to(target)?;
            }
            StackOp::Ret => {
                let target = self.pop_return(memory)?;
                self.jump_to(target)?;
            }
            StackOp::Load => {
                let address = self.pop(memory)?;
                let value = memory.read_word(address)?.as_i64();
                self.push(memory, value)?;
            }
            StackOp::Store => {
                let address = self.pop(memory)?;
                let value = self.pop(memory)?;
                memory.write_word(address, Word21::new(value))?;
                self.words_moved += 1;
            }
        }
        Ok(())
    }

    fn jump_to(&mut self, target: i64) -> Result<(), StackError> {
        if target < 0 {
            return Err(StackError::ProgramCounterOutOfBounds(0));
        }
        self.pc = target as usize;
        Ok(())
    }

    /// Runs until `HALT`, an error, or `max_steps`.
    pub fn run(
        &mut self,
        memory: &mut Memory,
        code: &[i64],
        max_steps: u64,
    ) -> Result<(), StackError> {
        while !self.halted {
            if self.steps >= max_steps {
                return Err(StackError::StepLimitExceeded(max_steps));
            }
            self.step(memory, code)?;
        }
        Ok(())
    }
}

/// Packs an operand into two balanced trytes.
#[must_use]
pub fn pack_operand(value: i64) -> [i64; OPERAND_TRYTES] {
    let low = wrap_balanced::<6>(value);
    let high = wrap_balanced::<6>((value - low) / 729);
    [low, high]
}

/// Builder for tryte-stream programs.
#[derive(Clone, Debug, Default)]
pub struct StackProgram {
    code: Vec<i64>,
}

impl StackProgram {
    /// Creates an empty program.
    #[must_use]
    pub fn new() -> Self {
        Self { code: Vec::new() }
    }

    /// Appends a bare opcode.
    #[must_use]
    pub fn op(mut self, op: StackOp) -> Self {
        self.code.push(op as i64);
        self
    }

    /// Appends an opcode followed by a two-tryte operand.
    #[must_use]
    pub fn op_with(mut self, op: StackOp, operand: i64) -> Self {
        self.code.push(op as i64);
        self.code.extend_from_slice(&pack_operand(operand));
        self
    }

    /// Appends a `PUSH` of `value`.
    #[must_use]
    pub fn push(self, value: i64) -> Self {
        self.op_with(StackOp::Push, value)
    }

    /// Returns the encoded tryte stream.
    #[must_use]
    pub fn build(self) -> Vec<i64> {
        self.code
    }

    /// Returns the current tryte index, for label resolution.
    #[must_use]
    pub fn position(&self) -> i64 {
        self.code.len() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{Cpu, Instruction, Opcode};

    fn emit(code: &mut Vec<i64>, op: StackOp) -> usize {
        code.push(op as i64);
        code.len() - 1
    }

    fn emit_with(code: &mut Vec<i64>, op: StackOp, operand: i64) -> usize {
        code.push(op as i64);
        let position = code.len();
        code.extend_from_slice(&pack_operand(operand));
        position
    }

    fn patch(code: &mut [i64], position: usize, operand: i64) {
        let [low, high] = pack_operand(operand);
        code[position] = low;
        code[position + 1] = high;
    }

    fn run_code(code: &[i64]) -> (StackMachine, Memory) {
        let mut memory = Memory::new();
        let mut machine = StackMachine::new();
        machine
            .run(&mut memory, code, 10_000)
            .expect("stack program runs");
        (machine, memory)
    }

    fn evaluate(code: &[i64]) -> i64 {
        let (mut machine, memory) = run_code(code);
        machine.pop(&memory).expect("result on the stack")
    }

    #[test]
    fn test_rpn_expression() {
        // ((5 - 2) * (8 + 4)) / 6 = 6
        let code = StackProgram::new()
            .push(5)
            .push(2)
            .op(StackOp::Sub)
            .push(8)
            .push(4)
            .op(StackOp::Add)
            .op(StackOp::Mul)
            .push(6)
            .op(StackOp::Div)
            .op(StackOp::Halt)
            .build();
        assert_eq!(evaluate(&code), 6);
    }

    #[test]
    fn test_rpn_factorial_loop() {
        // acc on the operand stack, counter n in MEM[700].
        let mut code = Vec::new();
        emit_with(&mut code, StackOp::Push, 1);
        emit_with(&mut code, StackOp::Push, 5);
        emit_with(&mut code, StackOp::Push, 700);
        emit(&mut code, StackOp::Store);
        let loop_start = code.len();
        emit_with(&mut code, StackOp::Push, 700);
        emit(&mut code, StackOp::Load);
        emit(&mut code, StackOp::Dup);
        let zero_test = emit_with(&mut code, StackOp::Jz, 0);
        emit(&mut code, StackOp::Mul);
        emit_with(&mut code, StackOp::Push, 700);
        emit(&mut code, StackOp::Load);
        emit_with(&mut code, StackOp::Push, 1);
        emit(&mut code, StackOp::Sub);
        emit_with(&mut code, StackOp::Push, 700);
        emit(&mut code, StackOp::Store);
        emit_with(&mut code, StackOp::Jmp, loop_start as i64);
        let end = code.len();
        emit(&mut code, StackOp::Pop);
        emit(&mut code, StackOp::Halt);
        patch(&mut code, zero_test, end as i64);

        assert_eq!(evaluate(&code), 120);
    }

    #[test]
    fn test_call_and_return_use_the_return_stack() {
        let mut code = Vec::new();
        emit_with(&mut code, StackOp::Push, 7);
        emit_with(&mut code, StackOp::Push, 20);
        let call_site = emit_with(&mut code, StackOp::Call, 0);
        emit(&mut code, StackOp::Add);
        emit(&mut code, StackOp::Halt);
        let subroutine = code.len();
        emit(&mut code, StackOp::Dup);
        emit(&mut code, StackOp::Add);
        emit(&mut code, StackOp::Ret);
        patch(&mut code, call_site, subroutine as i64);

        let (machine, memory) = run_code(&code);
        assert_eq!(machine.return_sp, RETURN_STACK_TOP);
        assert_eq!(evaluate(&code), 47);
        assert!(memory.read_word(RETURN_STACK_TOP).is_ok());
    }

    #[test]
    fn test_load_and_store() {
        let code = StackProgram::new()
            .push(42)
            .push(500)
            .op(StackOp::Store)
            .push(500)
            .op(StackOp::Load)
            .op(StackOp::Halt)
            .build();
        assert_eq!(evaluate(&code), 42);
    }

    #[test]
    fn test_operand_underflow_is_reported() {
        let code = StackProgram::new().op(StackOp::Add).build();
        let mut memory = Memory::new();
        let mut machine = StackMachine::new();
        assert_eq!(
            machine.run(&mut memory, &code, 10),
            Err(StackError::OperandUnderflow)
        );
    }

    #[test]
    fn test_division_by_zero_is_reported() {
        let code = StackProgram::new().push(1).push(0).op(StackOp::Div).build();
        let mut memory = Memory::new();
        let mut machine = StackMachine::new();
        assert_eq!(
            machine.run(&mut memory, &code, 10),
            Err(StackError::DivisionByZero)
        );
    }

    #[test]
    fn test_stacks_do_not_share_memory() {
        let code = StackProgram::new()
            .push(1)
            .push(2)
            .op(StackOp::Halt)
            .build();
        let (machine, _) = run_code(&code);
        assert_eq!(machine.operand_sp, OPERAND_STACK_TOP - 2);
        assert_eq!(machine.return_sp, RETURN_STACK_TOP);
    }

    #[test]
    fn test_rpn_uses_fewer_instructions_than_accumulator_for_immediates() {
        let stack_code = StackProgram::new()
            .push(5)
            .push(2)
            .op(StackOp::Sub)
            .push(8)
            .push(4)
            .op(StackOp::Add)
            .op(StackOp::Mul)
            .push(6)
            .op(StackOp::Div)
            .op(StackOp::Halt)
            .build();
        let (machine, memory) = run_code(&stack_code);
        let stack_result = {
            let mut copy = machine;
            copy.pop(&memory).unwrap()
        };

        let accumulator_program = [
            Instruction::new(Opcode::LoadImm, 0, 5),
            Instruction::new(Opcode::Store, 0, 800),
            Instruction::new(Opcode::LoadImm, 0, 2),
            Instruction::new(Opcode::Store, 0, 801),
            Instruction::new(Opcode::Load, 0, 800),
            Instruction::new(Opcode::Sub, 0, 801),
            Instruction::new(Opcode::Store, 0, 800),
            Instruction::new(Opcode::LoadImm, 0, 8),
            Instruction::new(Opcode::Store, 0, 801),
            Instruction::new(Opcode::LoadImm, 0, 4),
            Instruction::new(Opcode::Store, 0, 802),
            Instruction::new(Opcode::Load, 0, 801),
            Instruction::new(Opcode::Add, 0, 802),
            Instruction::new(Opcode::Store, 0, 801),
            Instruction::new(Opcode::Load, 0, 800),
            Instruction::new(Opcode::MulInt, 0, 801),
            Instruction::new(Opcode::Store, 0, 800),
            Instruction::new(Opcode::LoadImm, 0, 6),
            Instruction::new(Opcode::Store, 0, 801),
            Instruction::new(Opcode::Load, 0, 800),
            Instruction::new(Opcode::DivInt, 0, 801),
            Instruction::new(Opcode::Store, 0, 800),
            Instruction::new(Opcode::Halt, 0, 0),
        ];
        let mut accumulator_memory = Memory::new();
        let mut cpu = Cpu::new();
        cpu.load_program(&mut accumulator_memory, 0, &accumulator_program);
        let accumulator_cycles = cpu.run_ticks(&mut accumulator_memory, 1_000);
        let accumulator_result = accumulator_memory.read_word(800).unwrap().as_i64();

        println!(
            "RPN vs accumulator: stack steps = {} (words moved = {}), accumulator cycles = {}, \
             results = {} and {}",
            machine.steps,
            machine.words_moved,
            accumulator_cycles,
            stack_result,
            accumulator_result
        );

        assert_eq!(stack_result, 6);
        assert_eq!(accumulator_result, 6);
        assert!(
            machine.steps < accumulator_cycles as u64,
            "stack encoding should need fewer instructions for an immediate-heavy expression"
        );
    }
}
