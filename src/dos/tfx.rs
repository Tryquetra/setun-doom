//! Ternary Flat Executable (TFX) format.
//!
//! Compact binary executable container inspired by KolibriOS `APP_HEADER_01_`.
//! Encodes an 84-trit (4-word) header followed by contiguous Setun 21-trit instructions.

use crate::cpu::Instruction;
use crate::memory::Memory;
use crate::word::Word21;

/// Magic signature for Setun-DOS executable binaries: `TFX1` (1_993_121).
pub const TFX_MAGIC: i64 = 1_993_121;

/// Total header size in `Word21` words (4 words = 84 trits).
pub const TFX_HEADER_WORDS: usize = 4;

/// Represents a compiled flat executable ready for Setun-DOS execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TfxExecutable {
    /// Initial Program Counter after loading.
    pub entry_pc: i64,
    /// Code and initialized data words.
    pub words: Vec<Word21>,
    /// Initial Stack Pointer (`SP`).
    pub stack_top: i64,
}

impl TfxExecutable {
    /// Creates a new executable from raw Setun instructions.
    pub fn from_instructions(entry_pc: i64, instructions: &[Instruction], stack_top: i64) -> Self {
        let words = instructions.iter().map(|inst| inst.encode()).collect();
        Self {
            entry_pc,
            words,
            stack_top,
        }
    }

    /// Creates a new executable from encoded `Word21` words.
    pub fn from_raw_words(entry_pc: i64, words: Vec<Word21>, stack_top: i64) -> Self {
        Self {
            entry_pc,
            words,
            stack_top,
        }
    }

    /// Serializes the executable into a flat slice of `Word21` words including the 4-word header.
    pub fn to_words(&self) -> Vec<Word21> {
        let mut out = Vec::with_capacity(TFX_HEADER_WORDS + self.words.len());
        // Header Word 0: Magic
        out.push(Word21::new(TFX_MAGIC));
        // Header Word 1: Entry PC
        out.push(Word21::new(self.entry_pc));
        // Header Word 2: Code Size (number of words)
        out.push(Word21::new(self.words.len() as i64));
        // Header Word 3: Stack Top
        out.push(Word21::new(self.stack_top));

        // Payload words
        out.extend_from_slice(&self.words);
        out
    }

    /// Deserializes a TFX executable from a slice of `Word21` words.
    pub fn from_words(slice: &[Word21]) -> Result<Self, String> {
        if slice.len() < TFX_HEADER_WORDS {
            return Err(format!(
                "TFX image too small: expected at least {} words, found {}",
                TFX_HEADER_WORDS,
                slice.len()
            ));
        }

        let magic = slice[0].as_i64();
        if magic != TFX_MAGIC {
            return Err(format!(
                "Invalid TFX signature: expected {}, found {}",
                TFX_MAGIC, magic
            ));
        }

        let entry_pc = slice[1].as_i64();
        let code_len = slice[2].as_i64() as usize;
        let stack_top = slice[3].as_i64();

        if slice.len() < TFX_HEADER_WORDS + code_len {
            return Err(format!(
                "TFX image truncated: header declared {} words, image has {}",
                code_len,
                slice.len() - TFX_HEADER_WORDS
            ));
        }

        let words = slice[TFX_HEADER_WORDS..TFX_HEADER_WORDS + code_len].to_vec();
        Ok(Self {
            entry_pc,
            words,
            stack_top,
        })
    }

    /// Serializes the TFX executable into raw little-endian host bytes (8 bytes per Word21).
    pub fn to_bytes(&self) -> Vec<u8> {
        let words = self.to_words();
        let mut bytes = Vec::with_capacity(words.len() * 8);
        for w in words {
            bytes.extend_from_slice(&w.as_i64().to_le_bytes());
        }
        bytes
    }

    /// Deserializes a TFX executable from raw host bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() % 8 != 0 {
            return Err(format!(
                "Invalid byte length for TFX: expected multiple of 8, got {}",
                bytes.len()
            ));
        }

        let word_count = bytes.len() / 8;
        let mut words = Vec::with_capacity(word_count);
        for i in 0..word_count {
            let chunk = &bytes[i * 8..(i + 1) * 8];
            let val = i64::from_le_bytes(chunk.try_into().unwrap());
            words.push(Word21::new(val));
        }

        Self::from_words(&words)
    }

    /// Loads the executable into Setun memory starting at `load_base`.
    pub fn load_into_memory(&self, mem: &mut Memory, load_base: i64) -> Result<(), String> {
        for (i, &word) in self.words.iter().enumerate() {
            let addr = load_base + (i as i64);
            mem.write_word(addr, word).map_err(|e| {
                format!(
                    "Memory fault loading TFX at address {}: {:?}",
                    addr, e
                )
            })?;
        }
        Ok(())
    }
}
