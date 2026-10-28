// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! `ELF32` (`RV32I`) Binary Executable Loader and Mapping onto Ternary Linear Memory.
//!
//! This module implements a Little-Endian 32-bit `ELF` parser for the RISC-V architecture
//! (`EM_RISCV = 243`) and maps code segments (`.text`), constants (`.rodata`), initialized
//! data (`.data`), and zeroed data (`.bss`) directly onto the $3^{14} = 4,782,969$ word memory
//! space of the Setun machine.
//!
//! Each byte of the program binary image is transcoded into a balanced ternary [`Tryte6`]
//! (`[-128, +127]` inside `[-364, +364]`) and stored within a [`Word21`] cell in Setun memory,
//! preserving 1, 2, and 4-byte alignments for instructions `lb`, `lbu`, `lh`, `lhu`, `lw`, `sb`,
//! `sh`, and `sw`.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::memory::{Memory, MemoryError, MEMORY_SIZE};
use crate::word::{Tryte6, Word21};

/// Magic constant `\x7FELF` at the start of the ELF header.
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// 32-bit ELF class (`ELFCLASS32`).
pub const ELFCLASS32: u8 = 1;

/// Little-Endian encoding (`ELFDATA2LSB`).
pub const ELFDATA2LSB: u8 = 1;

/// RISC-V machine identifier (`EM_RISCV`).
pub const EM_RISCV: u16 = 243;

/// Loadable segment type (`PT_LOAD`).
pub const PT_LOAD: u32 = 1;

/// Allocatable section flag in memory (`SHF_ALLOC`).
pub const SHF_ALLOC: u32 = 0x2;

/// Executable section flag (`SHF_EXECINSTR`).
pub const SHF_EXECINSTR: u32 = 0x4;

/// Section type `SHT_NOBITS` (`.bss`).
pub const SHT_NOBITS: u32 = 8;

/// Section type `SHT_SYMTAB` (symbol table).
pub const SHT_SYMTAB: u32 = 2;

/// Default base address in Setun memory where the ELF virtual address space is mapped.
pub const DEFAULT_ELF_MEMORY_BASE: usize = 100_000;

/// Default stack top (`sp = x2`) in RV32I virtual address space (2 MB above the program base).
pub const DEFAULT_RV32I_STACK_TOP: u32 = 0x0020_0000;

/// Errors during reading, validation, or mapping of an `ELF32` binary.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElfError {
    /// File smaller than the minimum ELF32 header (52 bytes).
    TooShort(usize),
    /// Invalid magic signature `\x7FELF`.
    InvalidMagic([u8; 4]),
    /// ELF class different from 32-bit (`ELFCLASS32`).
    UnsupportedClass(u8),
    /// Endianness different from Little-Endian (`ELFDATA2LSB`).
    UnsupportedEndianness(u8),
    /// Target architecture different from RISC-V (`EM_RISCV = 243`).
    UnsupportedMachine(u16),
    /// Segment or section header table offset or size out of buffer bounds.
    MalformedHeader(&'static str),
    /// Error mapping bytes into Setun ternary memory.
    MemoryMap(MemoryError),
}

impl fmt::Display for ElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort(len) => write!(f, "File smaller than minimum ELF32 header: {len} bytes"),
            Self::InvalidMagic(magic) => write!(f, "Invalid ELF magic: {magic:02X?}"),
            Self::UnsupportedClass(c) => write!(f, "Unsupported ELF class (expected 32-bit): {c}"),
            Self::UnsupportedEndianness(e) => {
                write!(f, "Unsupported endianness (expected Little-Endian): {e}")
            }
            Self::UnsupportedMachine(m) => {
                write!(f, "Unsupported machine (expected EM_RISCV = 243): {m}")
            }
            Self::MalformedHeader(reason) => write!(f, "Malformed ELF header: {reason}"),
            Self::MemoryMap(err) => write!(f, "Memory mapping error: {err}"),
        }
    }
}

impl Error for ElfError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MemoryMap(err) => Some(err),
            _ => None,
        }
    }
}

impl From<MemoryError> for ElfError {
    fn from(err: MemoryError) -> Self {
        Self::MemoryMap(err)
    }
}

/// Loadable segment (`PT_LOAD`) extracted from the ELF32 program header.
#[derive(Clone, Debug)]
pub struct ElfSegment {
    /// Starting virtual address (`p_vaddr`).
    pub vaddr: u32,
    /// Memory size (`p_memsz`).
    pub mem_size: u32,
    /// Permission flags (`PF_X = 1`, `PF_W = 2`, `PF_R = 4`).
    pub flags: u32,
    /// Bytes loaded from file (`p_filesz`).
    pub data: Vec<u8>,
}

/// Named section extracted from the ELF32 section table.
#[derive(Clone, Debug)]
pub struct ElfSection {
    /// Section name (e.g., `.text`, `.data`, `.rodata`, `.bss`).
    pub name: String,
    /// Section type (`sh_type`).
    pub sh_type: u32,
    /// Section flags (`sh_flags`).
    pub sh_flags: u32,
    /// Virtual address (`sh_addr`).
    pub vaddr: u32,
    /// Size in bytes (`sh_size`).
    pub size: u32,
    /// Raw section data (empty for `SHT_NOBITS`).
    pub data: Vec<u8>,
}

/// Decoded `ELF32` image ready for loading into Setun memory.
#[derive(Clone, Debug)]
pub struct Elf32Image {
    /// Entry virtual address (`e_entry`).
    pub entry_point: u32,
    /// Binary `PT_LOAD` segments.
    pub segments: Vec<ElfSegment>,
    /// Allocatable sections (`SHF_ALLOC`).
    pub sections: Vec<ElfSection>,
    /// Symbol map (`name -> virtual address`).
    pub symbols: HashMap<String, u32>,
}

/// Summary of memory layout after loading an ELF32 image into Setun memory.
#[derive(Clone, Debug)]
pub struct LoadedElfLayout {
    /// Effective entry address (initial `PC` in RV32I virtual address space).
    pub entry_point: u32,
    /// Base offset in Setun linear memory (`0..MEMORY_SIZE`).
    pub setun_base_offset: usize,
    /// Lowest loaded virtual address.
    pub min_vaddr: u32,
    /// Highest loaded virtual address (exclusive).
    pub max_vaddr: u32,
    /// Total number of `Tryte6` cells written into Setun memory.
    pub total_trytes_written: usize,
    /// Executable code ranges (`(start_vaddr, end_vaddr)`).
    pub executable_ranges: Vec<(u32, u32)>,
}

impl Elf32Image {
    /// Decodes a byte buffer containing a RISC-V Little-Endian `ELF32` executable.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < 52 {
            return Err(ElfError::TooShort(bytes.len()));
        }
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != ELF_MAGIC {
            return Err(ElfError::InvalidMagic(magic));
        }
        if bytes[4] != ELFCLASS32 {
            return Err(ElfError::UnsupportedClass(bytes[4]));
        }
        if bytes[5] != ELFDATA2LSB {
            return Err(ElfError::UnsupportedEndianness(bytes[5]));
        }

        let e_machine = read_u16(bytes, 18)?;
        if e_machine != EM_RISCV {
            return Err(ElfError::UnsupportedMachine(e_machine));
        }

        let e_entry = read_u32(bytes, 24)?;
        let e_phoff = read_u32(bytes, 28)? as usize;
        let e_shoff = read_u32(bytes, 32)? as usize;
        let e_phentsize = read_u16(bytes, 42)? as usize;
        let e_phnum = read_u16(bytes, 44)? as usize;
        let e_shentsize = read_u16(bytes, 46)? as usize;
        let e_shnum = read_u16(bytes, 48)? as usize;
        let e_shstrndx = read_u16(bytes, 50)? as usize;

        let mut segments = Vec::new();
        if e_phoff > 0 && e_phnum > 0 {
            if e_phentsize < 32 {
                return Err(ElfError::MalformedHeader("e_phentsize < 32"));
            }
            for i in 0..e_phnum {
                let off = e_phoff
                    .checked_add(i * e_phentsize)
                    .ok_or(ElfError::MalformedHeader("phoff overflow"))?;
                if off + 32 > bytes.len() {
                    return Err(ElfError::MalformedHeader("program header out of bounds"));
                }
                let p_type = read_u32(bytes, off)?;
                if p_type != PT_LOAD {
                    continue;
                }
                let p_offset = read_u32(bytes, off + 4)? as usize;
                let p_vaddr = read_u32(bytes, off + 8)?;
                let p_filesz = read_u32(bytes, off + 16)? as usize;
                let p_memsz = read_u32(bytes, off + 20)?;
                let p_flags = read_u32(bytes, off + 24)?;

                let end_file = p_offset
                    .checked_add(p_filesz)
                    .ok_or(ElfError::MalformedHeader("segment filesz overflow"))?;
                if end_file > bytes.len() {
                    return Err(ElfError::MalformedHeader("segment data out of bounds"));
                }

                segments.push(ElfSegment {
                    vaddr: p_vaddr,
                    mem_size: p_memsz,
                    flags: p_flags,
                    data: bytes[p_offset..end_file].to_vec(),
                });
            }
        }

        let mut sections = Vec::new();
        let mut symbols = HashMap::new();

        if e_shoff > 0 && e_shnum > 0 && e_shentsize >= 40 {
            let shstr_slice = if e_shstrndx < e_shnum {
                let sh_off = e_shoff + e_shstrndx * e_shentsize;
                if sh_off + 40 <= bytes.len() {
                    let str_off = read_u32(bytes, sh_off + 16)? as usize;
                    let str_sz = read_u32(bytes, sh_off + 20)? as usize;
                    if str_off + str_sz <= bytes.len() {
                        &bytes[str_off..str_off + str_sz]
                    } else {
                        &[][..]
                    }
                } else {
                    &[][..]
                }
            } else {
                &[][..]
            };

            for i in 0..e_shnum {
                let off = e_shoff
                    .checked_add(i * e_shentsize)
                    .ok_or(ElfError::MalformedHeader("shoff overflow"))?;
                if off + 40 > bytes.len() {
                    return Err(ElfError::MalformedHeader("section header out of bounds"));
                }
                let sh_name_idx = read_u32(bytes, off)? as usize;
                let sh_type = read_u32(bytes, off + 4)?;
                let sh_flags = read_u32(bytes, off + 8)?;
                let sh_addr = read_u32(bytes, off + 12)?;
                let sh_offset = read_u32(bytes, off + 16)? as usize;
                let sh_size = read_u32(bytes, off + 20)?;
                let sh_link = read_u32(bytes, off + 24)? as usize;

                let name = read_cstring(shstr_slice, sh_name_idx);

                if (sh_flags & SHF_ALLOC) != 0 && sh_size > 0 {
                    let data = if sh_type == SHT_NOBITS {
                        Vec::new()
                    } else {
                        let end = sh_offset
                            .checked_add(sh_size as usize)
                            .ok_or(ElfError::MalformedHeader("section size overflow"))?;
                        if end > bytes.len() {
                            return Err(ElfError::MalformedHeader("section data out of bounds"));
                        }
                        bytes[sh_offset..end].to_vec()
                    };
                    sections.push(ElfSection {
                        name,
                        sh_type,
                        sh_flags,
                        vaddr: sh_addr,
                        size: sh_size,
                        data,
                    });
                } else if sh_type == SHT_SYMTAB && sh_link < e_shnum {
                    let str_sh_off = e_shoff + sh_link * e_shentsize;
                    if str_sh_off + 40 <= bytes.len() {
                        let str_off = read_u32(bytes, str_sh_off + 16)? as usize;
                        let str_sz = read_u32(bytes, str_sh_off + 20)? as usize;
                        let sym_end = sh_offset + (sh_size as usize);
                        if str_off + str_sz <= bytes.len() && sym_end <= bytes.len() {
                            let strtab = &bytes[str_off..str_off + str_sz];
                            let mut cur = sh_offset;
                            while cur + 16 <= sym_end {
                                let st_name = read_u32(bytes, cur)? as usize;
                                let st_value = read_u32(bytes, cur + 4)?;
                                let sym_name = read_cstring(strtab, st_name);
                                if !sym_name.is_empty() {
                                    symbols.insert(sym_name, st_value);
                                }
                                cur += 16;
                            }
                        }
                    }
                }
            }
        }

        Ok(Self {
            entry_point: e_entry,
            segments,
            sections,
            symbols,
        })
    }

    /// Loads the `ELF32` image into Setun linear memory starting at `base_offset`,
    /// converting each byte into a centered [`Tryte6`] inside a [`Word21`].
    pub fn load_into_setun_memory(
        &self,
        memory: &mut Memory,
        base_offset: usize,
    ) -> Result<LoadedElfLayout, ElfError> {
        let mut min_vaddr = u32::MAX;
        let mut max_vaddr = 0u32;
        let mut total_trytes_written = 0usize;
        let mut executable_ranges = Vec::new();

        // When allocatable sections (.text, .rodata, .data, .bss) are present,
        // load each section directly to avoid PHDR header overlap.
        if !self.sections.is_empty() {
            for sec in &self.sections {
                let start = sec.vaddr;
                let end = sec.vaddr.saturating_add(sec.size);
                min_vaddr = min_vaddr.min(start);
                max_vaddr = max_vaddr.max(end);

                if (sec.sh_flags & SHF_EXECINSTR) != 0 {
                    executable_ranges.push((start, end));
                }

                for i in 0..(sec.size as usize) {
                    let byte = if i < sec.data.len() { sec.data[i] } else { 0u8 };
                    let target_addr = base_offset
                        .checked_add(start as usize)
                        .and_then(|a| a.checked_add(i))
                        .ok_or(MemoryError::OutOfBounds(i64::MAX))?;
                    if target_addr >= MEMORY_SIZE {
                        return Err(ElfError::MemoryMap(MemoryError::OutOfBounds(
                            target_addr as i64,
                        )));
                    }
                    let tryte = Tryte6::from_u8_centered(byte);
                    memory.write_word(target_addr as i64, Word21::new(tryte.as_i64()))?;
                    total_trytes_written += 1;
                }
            }
        } else {
            for seg in &self.segments {
                let start = seg.vaddr;
                let end = seg.vaddr.saturating_add(seg.mem_size);
                min_vaddr = min_vaddr.min(start);
                max_vaddr = max_vaddr.max(end);

                if (seg.flags & 0x1) != 0 {
                    executable_ranges.push((start, end));
                }

                for i in 0..(seg.mem_size as usize) {
                    let byte = if i < seg.data.len() { seg.data[i] } else { 0u8 };
                    let target_addr = base_offset
                        .checked_add(start as usize)
                        .and_then(|a| a.checked_add(i))
                        .ok_or(MemoryError::OutOfBounds(i64::MAX))?;
                    if target_addr >= MEMORY_SIZE {
                        return Err(ElfError::MemoryMap(MemoryError::OutOfBounds(
                            target_addr as i64,
                        )));
                    }
                    let tryte = Tryte6::from_u8_centered(byte);
                    memory.write_word(target_addr as i64, Word21::new(tryte.as_i64()))?;
                    total_trytes_written += 1;
                }
            }
        }

        let entry_point = if self.entry_point != 0 {
            self.entry_point
        } else if let Some(&main_addr) = self.symbols.get("main") {
            main_addr
        } else if let Some(&start_addr) = self.symbols.get("_start") {
            start_addr
        } else {
            min_vaddr
        };

        Ok(LoadedElfLayout {
            entry_point,
            setun_base_offset: base_offset,
            min_vaddr,
            max_vaddr,
            total_trytes_written,
            executable_ranges,
        })
    }
}

/// Reads 1 byte stored as a centered [`Tryte6`] in Setun memory at `base_offset + vaddr`.
#[inline]
pub fn read_elf_u8(memory: &Memory, base_offset: usize, vaddr: u32) -> Result<u8, MemoryError> {
    let addr = base_offset
        .checked_add(vaddr as usize)
        .ok_or(MemoryError::OutOfBounds(vaddr as i64))?;
    let w = memory.read_word(addr as i64)?;
    let tryte = Tryte6::from_i64(w.as_i64());
    Ok(tryte.to_u8_centered())
}

/// Writes 1 byte as a centered [`Tryte6`] in Setun memory at `base_offset + vaddr`,
/// returning `(old_word21, new_word21)` for *Trit Flips* accounting.
#[inline]
pub fn write_elf_u8(
    memory: &mut Memory,
    base_offset: usize,
    vaddr: u32,
    val: u8,
) -> Result<(Word21, Word21), MemoryError> {
    let addr = base_offset
        .checked_add(vaddr as usize)
        .ok_or(MemoryError::OutOfBounds(vaddr as i64))?;
    let old_word = memory.read_word(addr as i64)?;
    let new_word = Word21::new(Tryte6::from_u8_centered(val).as_i64());
    memory.write_word(addr as i64, new_word)?;
    Ok((old_word, new_word))
}

/// Reads a 16-bit Little-Endian integer from 2 consecutive [`Tryte6`] cells in Setun memory.
#[inline]
pub fn read_elf_u16_le(
    memory: &Memory,
    base_offset: usize,
    vaddr: u32,
) -> Result<u16, MemoryError> {
    let b0 = read_elf_u8(memory, base_offset, vaddr)? as u16;
    let b1 = read_elf_u8(memory, base_offset, vaddr.wrapping_add(1))? as u16;
    Ok(b0 | (b1 << 8))
}

/// Reads a 32-bit Little-Endian integer from 4 consecutive [`Tryte6`] cells in Setun memory.
#[inline]
pub fn read_elf_u32_le(
    memory: &Memory,
    base_offset: usize,
    vaddr: u32,
) -> Result<u32, MemoryError> {
    let b0 = read_elf_u8(memory, base_offset, vaddr)? as u32;
    let b1 = read_elf_u8(memory, base_offset, vaddr.wrapping_add(1))? as u32;
    let b2 = read_elf_u8(memory, base_offset, vaddr.wrapping_add(2))? as u32;
    let b3 = read_elf_u8(memory, base_offset, vaddr.wrapping_add(3))? as u32;
    Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
}

#[inline]
fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ElfError> {
    if offset + 2 > bytes.len() {
        return Err(ElfError::MalformedHeader("u16 read out of bounds"));
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[inline]
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ElfError> {
    if offset + 4 > bytes.len() {
        return Err(ElfError::MalformedHeader("u32 read out of bounds"));
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_cstring(table: &[u8], offset: usize) -> String {
    if offset >= table.len() {
        return String::new();
    }
    let slice = &table[offset..];
    let len = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..len]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elf32_parse_and_tryte6_memory_load() {
        let elf_bytes = include_bytes!("../vendor/riscv_tests/elf/lw.elf");
        let image = Elf32Image::parse(elf_bytes).expect("Failed to parse lw.elf");

        assert_eq!(image.entry_point, 0x11100);
        assert!(image.symbols.contains_key("main"));
        assert!(image.symbols.contains_key("tdat"));

        let mut mem = Memory::new();
        let layout = image
            .load_into_setun_memory(&mut mem, DEFAULT_ELF_MEMORY_BASE)
            .expect("Failed to map ELF into Setun memory");

        assert_eq!(layout.entry_point, 0x11100);
        assert!(layout.total_trytes_written > 0);

        // Verify that data in the .data section (`tdat`) was stored as Tryte6 and reconstructs 0x00ff00ff
        let tdat_addr = *image.symbols.get("tdat").unwrap();
        let word0 = read_elf_u32_le(&mem, DEFAULT_ELF_MEMORY_BASE, tdat_addr).unwrap();
        let word1 = read_elf_u32_le(&mem, DEFAULT_ELF_MEMORY_BASE, tdat_addr + 4).unwrap();
        assert_eq!(word0, 0x00ff_00ff);
        assert_eq!(word1, 0xff00_ff00);
    }
}
