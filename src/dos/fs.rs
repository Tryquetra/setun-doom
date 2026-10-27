//! Virtual filesystem and WAD container loader for Setun-DOS.
//!
//! Exposes storage handles to user-space tasks through the `SYS_FILE_READ` system call.

use crate::word::Word21;

/// Represents an in-memory file or Doom WAD lump stored in Setun-DOS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualFile {
    pub name: String,
    pub words: Vec<Word21>,
}

/// Virtual filesystem managing open files and mounted WAD packages.
#[derive(Clone, Debug, Default)]
pub struct SetunFileSystem {
    pub files: Vec<VirtualFile>,
}

impl SetunFileSystem {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Adds an in-memory file with raw `Word21` data, returning its file handle descriptor.
    pub fn add_file(&mut self, name: &str, words: Vec<Word21>) -> usize {
        let handle = self.files.len();
        self.files.push(VirtualFile {
            name: name.to_string(),
            words,
        });
        handle
    }

    /// Adds a file converted from host binary bytes.
    pub fn add_bytes(&mut self, name: &str, bytes: &[u8]) -> usize {
        let words = bytes.iter().map(|&b| Word21::new(b as i64)).collect();
        self.add_file(name, words)
    }

    /// Searches for a file by name, returning its handle if found.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.files.iter().position(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// Mounts an authentic Doom IWAD/PWAD byte buffer, indexing lumps into virtual file descriptors.
    pub fn mount_wad(&mut self, wad_bytes: &[u8]) -> Result<usize, String> {
        if wad_bytes.len() < 12 {
            return Err("WAD buffer too small for header".to_string());
        }

        let magic = std::str::from_utf8(&wad_bytes[0..4])
            .map_err(|_| "Invalid UTF-8 in WAD magic header".to_string())?;
        if magic != "IWAD" && magic != "PWAD" {
            return Err(format!("Unrecognized WAD identification tag: '{}'", magic));
        }

        let num_lumps = i32::from_le_bytes(wad_bytes[4..8].try_into().unwrap()) as usize;
        let dir_offset = i32::from_le_bytes(wad_bytes[8..12].try_into().unwrap()) as usize;

        if dir_offset + num_lumps * 16 > wad_bytes.len() {
            return Err("WAD directory offset extends past end of file".to_string());
        }

        let mut lumps_mounted = 0;
        for i in 0..num_lumps {
            let entry_pos = dir_offset + i * 16;
            let lump_pos = i32::from_le_bytes(wad_bytes[entry_pos..entry_pos + 4].try_into().unwrap()) as usize;
            let lump_size = i32::from_le_bytes(wad_bytes[entry_pos + 4..entry_pos + 8].try_into().unwrap()) as usize;

            let name_bytes = &wad_bytes[entry_pos + 8..entry_pos + 16];
            let name_end = name_bytes.iter().position(|&c| c == 0).unwrap_or(8);
            let lump_name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();

            if lump_size > 0 && lump_pos + lump_size <= wad_bytes.len() {
                let data = &wad_bytes[lump_pos..lump_pos + lump_size];
                self.add_bytes(&lump_name, data);
                lumps_mounted += 1;
            }
        }

        // Also add the raw WAD file as handle 0 if not present
        self.add_bytes("DOOM.WAD", wad_bytes);

        Ok(lumps_mounted)
    }

    /// Exposes file storage as a slice of tuples for the syscall dispatcher.
    pub fn as_storage_slice(&self) -> Vec<(String, Vec<Word21>)> {
        self.files.iter().map(|f| (f.name.clone(), f.words.clone())).collect()
    }
}
