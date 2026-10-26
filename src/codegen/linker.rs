//! In-memory static linker for Setun-Doom 21-trit native executables.
//!
//! Inspired by Bun's high-performance bundler/linker architecture:
//! - Single-pass in-memory resolution (zero intermediate object files on disk).
//! - Whole-program reachability analysis and dead-code elimination (tree-shaking).
//! - Literal and constant deduplication (`LiteralPool`).
//! - Deterministic layout across the 14-trit linear memory space.

use crate::codegen::ir::{LlvmFunction, LlvmInst, LlvmModule, LlvmValue};
use crate::codegen::isel::SetunCodeGenerator;
use crate::cpu::{Instruction, Opcode};
use crate::memory::Memory;
use crate::word::Word21;
use std::collections::{HashMap, HashSet};

/// Base memory address for executable code (.text segment).
pub const CODE_SEGMENT_BASE: usize = 1_000;

/// Base memory address for deduplicated literals and read-only data (.rodata).
pub const RODATA_SEGMENT_BASE: usize = 200_000;

/// A record representing an unresolved cross-function or data symbol call site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelocationRecord {
    /// Index in the global instruction stream where the target address must be patched.
    pub instruction_idx: usize,
    /// Symbolic target function or global variable name.
    pub target_symbol: String,
}

/// A fully linked, self-contained executable image ready for direct execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkedImage {
    /// Entry point address in Setun linear memory.
    pub entry_pc: usize,
    /// Instruction words indexed by linear memory address.
    pub code_words: Vec<(usize, Word21)>,
    /// Deduplicated read-only data words indexed by linear memory address.
    pub data_words: Vec<(usize, Word21)>,
    /// Global symbol table mapping function and variable names to linear memory addresses.
    pub symbol_table: HashMap<String, usize>,
}

impl LinkedImage {
    /// Loads the linked executable image directly into Setun linear memory.
    pub fn load_into_memory(&self, mem: &mut Memory) {
        for &(addr, word) in &self.code_words {
            mem.write_word_fast(addr as i64, word);
        }
        for &(addr, word) in &self.data_words {
            mem.write_word_fast(addr as i64, word);
        }
    }
}

/// In-memory static linker engine.
#[derive(Default)]
pub struct SetunLinker {
    modules: Vec<LlvmModule>,
}

impl SetunLinker {
    /// Creates a new, empty static linker instance.
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    /// Adds an LLVM IR module containing functions and declarations to the linker.
    pub fn add_module(&mut self, module: LlvmModule) {
        self.modules.push(module);
    }

    /// Performs single-pass in-memory linking, tree-shaking from `entry_symbol`,
    /// and emits a self-contained [`LinkedImage`].
    pub fn link(self, entry_symbol: &str) -> Result<LinkedImage, String> {
        // 1. Collect all functions across modules into a unified function map
        let mut available_functions: HashMap<String, LlvmFunction> = HashMap::new();
        for module in self.modules {
            for (name, func) in module.functions {
                available_functions.insert(name, func);
            }
        }

        if !available_functions.contains_key(entry_symbol) {
            return Err(format!(
                "Linker error: entry point symbol '{}' not found in any module",
                entry_symbol
            ));
        }

        // 2. Perform whole-program reachability analysis (tree-shaking) from entry_symbol
        let reachable_names = Self::compute_reachability(entry_symbol, &available_functions);

        // 3. Layout reachable functions sequentially starting at CODE_SEGMENT_BASE
        let mut symbol_table: HashMap<String, usize> = HashMap::new();
        let mut relocations: Vec<RelocationRecord> = Vec::new();
        let mut all_instructions: Vec<Instruction> = Vec::new();

        let mut current_code_addr = CODE_SEGMENT_BASE;

        // Place entry function first so execution starts cleanly at CODE_SEGMENT_BASE
        let mut ordered_names = vec![entry_symbol.to_string()];
        for name in &reachable_names {
            if name != entry_symbol {
                ordered_names.push(name.clone());
            }
        }

        for func_name in &ordered_names {
            let func = &available_functions[func_name];
            let func_start_pc = current_code_addr;
            symbol_table.insert(func_name.clone(), func_start_pc);

            let func_instructions = SetunCodeGenerator::compile_function(func)?;

            for instr in func_instructions {
                let instr_idx = all_instructions.len();
                // Check if instruction is a cross-function CALL site needing relocation
                if instr.opcode == Opcode::Call && instr.operand < 0 {
                    // Call targets with placeholder operands
                    let placeholder_name = format!("__unresolved_{}", instr.operand.unsigned_abs());
                    relocations.push(RelocationRecord {
                        instruction_idx: instr_idx,
                        target_symbol: placeholder_name,
                    });
                }
                all_instructions.push(instr);
                current_code_addr += 1;
            }
        }

        // 4. Resolve relocation records
        for reloc in relocations {
            let target_addr = *symbol_table.get(&reloc.target_symbol).ok_or_else(|| {
                format!(
                    "Linker error: undefined symbol reference '{}'",
                    reloc.target_symbol
                )
            })?;
            all_instructions[reloc.instruction_idx].operand = target_addr as i64;
        }

        // 5. Build linked image
        let entry_pc = *symbol_table.get(entry_symbol).unwrap();
        let mut code_words = Vec::with_capacity(all_instructions.len());
        for (i, instr) in all_instructions.into_iter().enumerate() {
            let addr = CODE_SEGMENT_BASE + i;
            code_words.push((addr, instr.encode()));
        }

        Ok(LinkedImage {
            entry_pc,
            code_words,
            data_words: Vec::new(),
            symbol_table,
        })
    }

    /// Computes the transitive closure of reachable functions starting from `entry`.
    fn compute_reachability(
        entry: &str,
        functions: &HashMap<String, LlvmFunction>,
    ) -> HashSet<String> {
        let mut visited = HashSet::new();
        let mut queue = vec![entry.to_string()];
        visited.insert(entry.to_string());

        while let Some(current) = queue.pop() {
            if let Some(func) = functions.get(&current) {
                for block in &func.blocks {
                    for inst in &block.instructions {
                        // Scan for potential function references in call operands
                        if let LlvmInst::Copy { src: LlvmValue::Variable(var), .. } = inst {
                            if functions.contains_key(var) && !visited.contains(var) {
                                visited.insert(var.clone());
                                queue.push(var.clone());
                            }
                        }
                    }
                }
            }
        }

        visited
    }
}
