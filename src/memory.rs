//! Linear Balanced Ternary Memory Space ($3^{14} = 4,782,969$ words) and VGA Framebuffer Mapping.
//!
//! The 14-trit balanced addressing space can be accessed via:
//! - Non-negative linear addresses `0 .. 4_782_969`.
//! - Symmetrically balanced ternary addresses `[-2_391_484 .. +2_391_484]`.
//!
//! Memory Layout (`Setun-Doom`):
//! - `0x000000 .. 4_600_000`: General RAM (Code, Data, BSS, Heap, and Stack).
//! - `4_600_000 .. 4_664_000` (`FRAMEBUFFER_BASE .. +64_000`): 320x200 VGA Framebuffer,
//!   where each word stores a 6-trit `Tryte6` (`[-364, +364]`) corresponding
//!   to pixel color palette index.
//! - `4_664_000 .. 4_664_256` (`PALETTE_BASE .. +256`): 256 packed RGB color palette entries.
//! - `4_664_256 .. 4_664_512` (`MMIO_BASE`): Memory-mapped I/O registers (Timer, Keyboard, DMA).
//! - `4_782_968` (`STACK_TOP`): Initial default top for Stack Pointer (`SP`).

use core::fmt;
#[cfg(feature = "std")]
use std::error::Error;

use crate::word::{Tryte6, Word21, HALF_POW3, POW3};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Total capacity of 14-trit addressing space: $3^{14} = 4,782,969$ words.
pub const MEMORY_SIZE: usize = POW3[14] as usize; // 4_782_969

/// Positive bound of a 14-trit balanced address (`+2_391_484`).
pub const BALANCED_ADDR_HALF: i64 = HALF_POW3[14]; // 2_391_484

/// Classic Doom horizontal resolution (VGA Mode 13h).
pub const FB_WIDTH: usize = 320;

/// Classic Doom vertical resolution (VGA Mode 13h).
pub const FB_HEIGHT: usize = 200;

/// Total pixel count in framebuffer (`320 * 200 = 64,000`).
pub const FB_SIZE: usize = FB_WIDTH * FB_HEIGHT; // 64_000

/// Base linear address of memory-mapped VGA Framebuffer region.
pub const FRAMEBUFFER_BASE: usize = 4_600_000;

/// Final (exclusive) linear address of VGA Framebuffer.
pub const FRAMEBUFFER_END: usize = FRAMEBUFFER_BASE + FB_SIZE; // 4_664_000

/// Base linear address of VGA palette table (256 entries).
pub const PALETTE_BASE: usize = FRAMEBUFFER_END; // 4_664_000

/// Base linear address of MMIO registers (Timer, Keyboard, DMA Status).
pub const MMIO_BASE: usize = PALETTE_BASE + 256; // 4_664_256
pub const MMIO_TICKS_MS: usize = MMIO_BASE;
pub const MMIO_KEY_STATE: usize = MMIO_BASE + 1;
pub const MMIO_WAD_SIZE: usize = MMIO_BASE + 2;

/// Default base linear address for loading WAD files via DMA.
pub const WAD_DMA_BASE: usize = 500_000;

/// Initial top of stack (`SP`), growing downward toward lower addresses.
pub const INITIAL_STACK_POINTER: i64 = 4_599_999;

/// Access errors for the ternary memory subsystem.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MemoryError {
    /// Address out of $3^{14}$ bounds (`0..4_782_969`).
    OutOfBounds(i64),
    /// DMA transfer exceeds available memory capacity.
    DmaOverflow { base: usize, len: usize },
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds(addr) => {
                write!(
                    f,
                    "Memory address {addr} is out of bounds (0..{MEMORY_SIZE})"
                )
            }
            Self::DmaOverflow { base, len } => {
                write!(f, "DMA transfer overflow at base {base} with length {len}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl Error for MemoryError {}

/// Linear memory of Setun-Doom architecture allocated in contiguous host `i64` vector.
pub struct Memory {
    /// Vector of `4,782,969` 64-bit words containing `Word21` values.
    cells: Vec<i64>,
    /// 256-color RGBA palette (`[R, G, B, A]`) for fast translation in `DG_DrawFrame`.
    pub palette_rgba: [[u8; 4]; 256],
}

impl Memory {
    /// Allocates and initializes $3^{14}$-word main memory and default Doom VGA palette.
    #[must_use]
    pub fn new() -> Self {
        let mut mem = Self {
            cells: vec![0i64; MEMORY_SIZE],
            palette_rgba: [[0, 0, 0, 255]; 256],
        };
        mem.init_default_palette();
        mem
    }

    /// Initializes a 256-color Doom-style palette (fire, crimson, metal, and gray shades)
    /// which can be overwritten by lump `PLAYPAL` from `DOOM.WAD`.
    fn init_default_palette(&mut self) {
        for i in 0..256 {
            let idx = i as u8;
            // Rich deterministic gradient covering dark tones, reds, and golds
            let r = idx.wrapping_mul(3).wrapping_add(idx >> 1);
            let g = idx.wrapping_mul(2);
            let b = idx ^ 0x55;
            self.palette_rgba[i] = [r, g, b, 255];
            let packed_rgb = ((r as i64) << 16) | ((g as i64) << 8) | (b as i64);
            self.cells[PALETTE_BASE + i] = Word21::new(packed_rgb).as_i64();
        }
    }

    /// Converts a linear or balanced address to a valid index in `0..MEMORY_SIZE`.
    ///
    /// Supports direct linear addressing `0..4_782_969` (standard for vectors and code)
    /// and, for negative values in `[-2_391_484..-1]`, modular wrap-around within $3^{14}$.
    #[inline(always)]
    pub const fn resolve_address(addr: i64) -> Result<usize, MemoryError> {
        if addr >= 0 && (addr as usize) < MEMORY_SIZE {
            Ok(addr as usize)
        } else if addr >= -BALANCED_ADDR_HALF && addr < 0 {
            Ok((addr + (MEMORY_SIZE as i64)) as usize)
        } else {
            Err(MemoryError::OutOfBounds(addr))
        }
    }

    /// Reads a `Word21` from the specified address.
    #[inline(always)]
    pub fn read_word(&self, addr: i64) -> Result<Word21, MemoryError> {
        let idx = Self::resolve_address(addr)?;
        Ok(Word21::new(self.cells[idx]))
    }

    /// Fast memory read requiring a valid 14-trit address (panics if out of bounds,
    /// prohibiting silent wrap-around).
    #[inline(always)]
    pub fn read_word_fast(&self, addr: i64) -> Word21 {
        let idx = Self::resolve_address(addr)
            .unwrap_or_else(|e| panic!("Read fault in ternary memory: {:?}", e));
        Word21::new(self.cells[idx])
    }

    /// Writes a `Word21` to the specified address.
    /// If address falls within the VGA Framebuffer range (`4_600_000..4_664_000`),
    /// value is normalized to a 6-trit balanced `Tryte6` (`[-364, +364]`).
    #[inline(always)]
    pub fn write_word(&mut self, addr: i64, val: Word21) -> Result<(), MemoryError> {
        let idx = Self::resolve_address(addr)?;
        if (FRAMEBUFFER_BASE..FRAMEBUFFER_END).contains(&idx) {
            let tryte = Tryte6::from_i64(val.as_i64());
            self.cells[idx] = tryte.as_i64();
        } else {
            self.cells[idx] = val.as_i64();
        }
        Ok(())
    }

    /// Fast memory write on CPU critical path requiring a valid 14-trit address
    /// (panics if out of bounds, prohibiting silent out-of-range write).
    #[inline(always)]
    pub fn write_word_fast(&mut self, addr: i64, val: Word21) {
        let idx = Self::resolve_address(addr)
            .unwrap_or_else(|e| panic!("Write fault in ternary memory: {:?}", e));
        if (FRAMEBUFFER_BASE..FRAMEBUFFER_END).contains(&idx) {
            self.cells[idx] = Tryte6::from_i64(val.as_i64()).as_i64();
        } else {
            self.cells[idx] = val.as_i64();
        }
    }

    /// Writes a pixel to position `(x, y)` in the framebuffer (`320x200`) using a `Tryte6`.
    #[inline(always)]
    pub fn write_pixel_tryte(
        &mut self,
        x: usize,
        y: usize,
        color: Tryte6,
    ) -> Result<(), MemoryError> {
        if x >= FB_WIDTH || y >= FB_HEIGHT {
            return Err(MemoryError::OutOfBounds((y * FB_WIDTH + x) as i64));
        }
        let offset = y * FB_WIDTH + x;
        self.cells[FRAMEBUFFER_BASE + offset] = color.as_i64();
        Ok(())
    }

    /// Reads the 6-trit `Tryte6` stored at framebuffer coordinate `(x, y)`.
    #[inline(always)]
    pub fn read_pixel_tryte(&self, x: usize, y: usize) -> Result<Tryte6, MemoryError> {
        if x >= FB_WIDTH || y >= FB_HEIGHT {
            return Err(MemoryError::OutOfBounds((y * FB_WIDTH + x) as i64));
        }
        let offset = y * FB_WIDTH + x;
        Ok(Tryte6::from_i64(self.cells[FRAMEBUFFER_BASE + offset]))
    }

    /// Sets a color in the VGA palette (`0..=255`) and synchronizes mapped `PALETTE_BASE`.
    #[inline]
    pub fn set_palette_entry(&mut self, index: u8, r: u8, g: u8, b: u8) {
        let i = index as usize;
        self.palette_rgba[i] = [r, g, b, 255];
        let packed = ((r as i64) << 16) | ((g as i64) << 8) | (b as i64);
        self.cells[PALETTE_BASE + i] = Word21::new(packed).as_i64();
    }

    /// Loads a 768-byte Doom `PLAYPAL` palette (`256 * 3` RGB bytes).
    pub fn load_playpal(&mut self, rgb_bytes: &[u8]) {
        let count = (rgb_bytes.len() / 3).min(256);
        for i in 0..count {
            let r = rgb_bytes[i * 3];
            let g = rgb_bytes[i * 3 + 1];
            let b = rgb_bytes[i * 3 + 2];
            self.set_palette_entry(i as u8, r, g, b);
        }
    }

    /// Returns the raw slice of `64,000` words from the ternary framebuffer.
    #[inline(always)]
    pub fn framebuffer_slice(&self) -> &[i64] {
        &self.cells[FRAMEBUFFER_BASE..FRAMEBUFFER_END]
    }

    /// Renders `64,000` ternary framebuffer entries (`Tryte6`) into an RGBA buffer of
    /// `320 * 200 * 4 = 256,000` bytes (`u8`) or `64,000` `u32` pixels (`0xAABBGGRR` / `0xFFRRGGBB`).
    pub fn render_framebuffer_rgba(&self, out_rgba: &mut [u8]) {
        assert!(out_rgba.len() >= FB_SIZE * 4);
        let fb = self.framebuffer_slice();
        #[cfg(all(feature = "std", target_arch = "x86_64"))]
        {
            crate::simd::palette_rgba(out_rgba, fb, &self.palette_rgba);
        }
        #[cfg(not(all(feature = "std", target_arch = "x86_64")))]
        {
            for (i, &cell) in fb.iter().enumerate() {
                let tryte = Tryte6::from_i64(cell);
                let pal_idx = tryte.to_palette_index() as usize;
                let rgba = self.palette_rgba[pal_idx];
                let base = i * 4;
                out_rgba[base] = rgba[0];
                out_rgba[base + 1] = rgba[1];
                out_rgba[base + 2] = rgba[2];
                out_rgba[base + 3] = rgba[3];
            }
        }
    }

    /// Direct access to the complete slice of memory cells.
    #[inline(always)]
    pub fn raw_cells(&self) -> &[i64] {
        &self.cells
    }

    /// Direct mutable access to the complete slice of memory cells.
    #[inline(always)]
    pub fn raw_cells_mut(&mut self) -> &mut [i64] {
        &mut self.cells
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_capacity_and_addressing() {
        let mut mem = Memory::new();
        assert_eq!(mem.raw_cells().len(), 4_782_969);

        let w = Word21::from_i64(-987_654_321);
        mem.write_word(123_456, w).unwrap();
        assert_eq!(mem.read_word(123_456).unwrap(), w);
    }

    #[test]
    fn test_framebuffer_tryte6_mapping_and_rgba_translation() {
        let mut mem = Memory::new();
        mem.set_palette_entry(200, 255, 64, 32);

        // Writes palette color 200 as centered Tryte6 (200 - 128 = +72) at pixel (10, 5)
        let color_tryte = Tryte6::from_palette_index(200);
        assert!(color_tryte.as_i16() >= -364 && color_tryte.as_i16() <= 364);

        let linear_addr = (FRAMEBUFFER_BASE + 5 * FB_WIDTH + 10) as i64;
        mem.write_word(linear_addr, Word21::from_i64(color_tryte.as_i64()))
            .unwrap();

        let read_back = mem.read_pixel_tryte(10, 5).unwrap();
        assert_eq!(read_back, color_tryte);
        assert_eq!(read_back.to_palette_index(), 200);

        // Translates to RGBA and verifies pixel (10, 5)
        let mut rgba = vec![0u8; FB_SIZE * 4];
        mem.render_framebuffer_rgba(&mut rgba);
        let px_offset = (5 * FB_WIDTH + 10) * 4;
        assert_eq!(&rgba[px_offset..px_offset + 4], &[255, 64, 32, 255]);
    }
}
