//! Data memory for the REBEL-6 core.
//!
//! The reference R2R simulator addresses data in tryte-sized cells and stores binary values
//! one byte per cell (`TrytesToInt` assembles `cell & 0xFF` little-endian). The core follows
//! that model: addresses are tryte (byte) indices, cells hold `0..=255` for binary accesses and
//! `-364..=364` for ternary accesses.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Data memory fault.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MemError {
    /// Address plus length falls outside the populated window.
    OutOfBounds {
        /// First address of the access.
        addr: i64,
        /// Number of cells accessed.
        len: usize,
    },
}

/// Byte/tryte addressed data memory.
#[derive(Clone, Debug)]
pub struct DataMem {
    cells: Vec<i16>,
}

impl DataMem {
    /// Creates a zeroed data memory with `size` cells.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self {
            cells: vec![0; size],
        }
    }

    /// Number of cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Returns true when the memory has no cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Returns the raw cells.
    #[must_use]
    pub fn cells(&self) -> &[i16] {
        &self.cells
    }

    /// Returns the raw cells mutably.
    #[must_use]
    pub fn cells_mut(&mut self) -> &mut [i16] {
        &mut self.cells
    }

    fn index(&self, addr: i64, len: usize) -> Result<usize, MemError> {
        if addr < 0 {
            return Err(MemError::OutOfBounds { addr, len });
        }
        let start = addr as usize;
        let end = start
            .checked_add(len)
            .ok_or(MemError::OutOfBounds { addr, len })?;
        if end > self.cells.len() {
            return Err(MemError::OutOfBounds { addr, len });
        }
        Ok(start)
    }

    /// Loads one cell as an unsigned byte.
    pub fn load_u8(&self, addr: i64) -> Result<u8, MemError> {
        let i = self.index(addr, 1)?;
        Ok((self.cells[i] & 0xFF) as u8)
    }

    /// Loads one cell as a signed byte.
    pub fn load_i8(&self, addr: i64) -> Result<i8, MemError> {
        Ok(self.load_u8(addr)? as i8)
    }

    /// Loads two cells as an unsigned little-endian halfword.
    pub fn load_u16(&self, addr: i64) -> Result<u16, MemError> {
        let i = self.index(addr, 2)?;
        Ok(((self.cells[i] & 0xFF) as u16) | (((self.cells[i + 1] & 0xFF) as u16) << 8))
    }

    /// Loads two cells as a signed little-endian halfword.
    pub fn load_i16(&self, addr: i64) -> Result<i16, MemError> {
        Ok(self.load_u16(addr)? as i16)
    }

    /// Loads four cells as an unsigned little-endian word.
    pub fn load_u32(&self, addr: i64) -> Result<u32, MemError> {
        let i = self.index(addr, 4)?;
        Ok(((self.cells[i] & 0xFF) as u32)
            | (((self.cells[i + 1] & 0xFF) as u32) << 8)
            | (((self.cells[i + 2] & 0xFF) as u32) << 16)
            | (((self.cells[i + 3] & 0xFF) as u32) << 24))
    }

    /// Loads four cells as a signed little-endian word.
    pub fn load_i32(&self, addr: i64) -> Result<i32, MemError> {
        Ok(self.load_u32(addr)? as i32)
    }

    /// Loads one ternary tryte.
    pub fn load_tryte(&self, addr: i64) -> Result<i16, MemError> {
        let i = self.index(addr, 1)?;
        Ok(self.cells[i])
    }

    /// Stores the low byte of `value` in one cell.
    pub fn store_u8(&mut self, addr: i64, value: u8) -> Result<(), MemError> {
        let i = self.index(addr, 1)?;
        self.cells[i] = i16::from(value);
        Ok(())
    }

    /// Stores the low halfword of `value` little-endian.
    pub fn store_u16(&mut self, addr: i64, value: u16) -> Result<(), MemError> {
        let i = self.index(addr, 2)?;
        self.cells[i] = (value & 0xFF) as i16;
        self.cells[i + 1] = ((value >> 8) & 0xFF) as i16;
        Ok(())
    }

    /// Stores `value` little-endian in four cells.
    pub fn store_u32(&mut self, addr: i64, value: u32) -> Result<(), MemError> {
        let i = self.index(addr, 4)?;
        self.cells[i] = (value & 0xFF) as i16;
        self.cells[i + 1] = ((value >> 8) & 0xFF) as i16;
        self.cells[i + 2] = ((value >> 16) & 0xFF) as i16;
        self.cells[i + 3] = ((value >> 24) & 0xFF) as i16;
        Ok(())
    }

    /// Stores a ternary tryte.
    pub fn store_tryte(&mut self, addr: i64, value: i16) -> Result<(), MemError> {
        let i = self.index(addr, 1)?;
        self.cells[i] = value;
        Ok(())
    }

    /// Copies bytes into memory.
    pub fn write_bytes(&mut self, addr: i64, bytes: &[u8]) -> Result<(), MemError> {
        let start = self.index(addr, bytes.len())?;
        for (i, &b) in bytes.iter().enumerate() {
            self.cells[start + i] = i16::from(b);
        }
        Ok(())
    }

    /// Reads bytes from memory.
    pub fn read_bytes(&self, addr: i64, len: usize) -> Result<Vec<u8>, MemError> {
        let start = self.index(addr, len)?;
        Ok(self.cells[start..start + len]
            .iter()
            .map(|&c| (c & 0xFF) as u8)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn little_endian_word_round_trip() {
        let mut mem = DataMem::new(64);
        mem.store_u32(4, 0x1234_5678).unwrap();
        assert_eq!(mem.load_u8(4).unwrap(), 0x78);
        assert_eq!(mem.load_u16(4).unwrap(), 0x5678);
        assert_eq!(mem.load_u32(4).unwrap(), 0x1234_5678);
        assert_eq!(mem.load_i32(4).unwrap(), 0x1234_5678);
    }

    #[test]
    fn signed_loads_match_binary_semantics() {
        let mut mem = DataMem::new(8);
        mem.store_u8(0, 0xC8).unwrap();
        assert_eq!(mem.load_u8(0).unwrap(), 200);
        assert_eq!(mem.load_i8(0).unwrap(), -56);
        mem.store_u16(2, 0x8001).unwrap();
        assert_eq!(mem.load_u16(2).unwrap(), 0x8001);
        assert_eq!(mem.load_i16(2).unwrap(), -32_767);
    }

    #[test]
    fn rejects_out_of_bounds() {
        let mem = DataMem::new(4);
        assert_eq!(
            mem.load_u32(2),
            Err(MemError::OutOfBounds { addr: 2, len: 4 })
        );
        assert_eq!(
            mem.load_u8(-1),
            Err(MemError::OutOfBounds { addr: -1, len: 1 })
        );
    }
}
