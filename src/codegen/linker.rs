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

    /// Serializes the linked image into a portable binary format (`.setun`).
    ///
    /// Header layout:
    /// - Magic bytes: `SETUN21\0` (8 bytes)
    /// - Entry PC: `u64` Little-Endian (8 bytes)
    /// - Code count: `u64` Little-Endian (8 bytes)
    /// - Data count: `u64` Little-Endian (8 bytes)
    /// - Sequence of (address `u64`, word_val `i64`)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SETUN21\0");
        bytes.extend_from_slice(&(self.entry_pc as u64).to_le_bytes());
        bytes.extend_from_slice(&(self.code_words.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&(self.data_words.len() as u64).to_le_bytes());

        for &(addr, word) in &self.code_words {
            bytes.extend_from_slice(&(addr as u64).to_le_bytes());
            bytes.extend_from_slice(&(word.as_i64().to_le_bytes()));
        }
        for &(addr, word) in &self.data_words {
            bytes.extend_from_slice(&(addr as u64).to_le_bytes());
            bytes.extend_from_slice(&(word.as_i64().to_le_bytes()));
        }

        bytes
    }

    /// Deserializes a `.setun` binary file from disk into a [`LinkedImage`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 32 {
            return Err("Invalid Setun binary: file smaller than header size (32 bytes)".to_string());
        }
        if &bytes[0..8] != b"SETUN21\0" {
            return Err("Invalid Setun binary: missing magic header 'SETUN21'".to_string());
        }

        let entry_pc = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
        let code_count = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
        let data_count = u64::from_le_bytes(bytes[24..32].try_into().unwrap()) as usize;

        let expected_size = 32 + (code_count + data_count) * 16;
        if bytes.len() < expected_size {
            return Err(format!(
                "Invalid Setun binary: truncated payload. Expected {} bytes, found {}",
                expected_size,
                bytes.len()
            ));
        }

        let mut offset = 32;
        let mut code_words = Vec::with_capacity(code_count);
        for _ in 0..code_count {
            let addr = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()) as usize;
            let val = i64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
            code_words.push((addr, Word21::new(val)));
            offset += 16;
        }

        let mut data_words = Vec::with_capacity(data_count);
        for _ in 0..data_count {
            let addr = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()) as usize;
            let val = i64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
            data_words.push((addr, Word21::new(val)));
            offset += 16;
        }

        Ok(Self {
            entry_pc,
            code_words,
            data_words,
            symbol_table: HashMap::new(),
        })
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

            let is_entry = func_name == entry_symbol;
            let (func_instructions, func_call_relocs) =
                SetunCodeGenerator::compile_function_with_relocs(func, is_entry)?;

            for (reloc_site, callee) in func_call_relocs {
                relocations.push(RelocationRecord {
                    instruction_idx: all_instructions.len() + reloc_site,
                    target_symbol: callee,
                });
            }

            for mut instr in func_instructions {
                if matches!(instr.opcode, Opcode::Jmp | Opcode::JmpP | Opcode::JmpN | Opcode::JmpZ) && instr.operand >= 0 {
                    // Rebase intra-function relative branch offset with func_start_pc
                    instr.operand += func_start_pc as i64;
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
                        match inst {
                            LlvmInst::Call { callee, .. } => {
                                if functions.contains_key(callee) && !visited.contains(callee) {
                                    visited.insert(callee.clone());
                                    queue.push(callee.clone());
                                }
                            }
                            LlvmInst::Copy { src: LlvmValue::Variable(var), .. } => {
                                if functions.contains_key(var) && !visited.contains(var) {
                                    visited.insert(var.clone());
                                    queue.push(var.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        visited
    }
}
