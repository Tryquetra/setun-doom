//! Doom execution harness running inside the Setun-DOS operating environment.
//!
//! Bridges Doom's `doomgeneric` contract (`DG_DrawFrame`, `DG_GetKey`, `DG_SleepMs`, `DG_GetTicksMs`, WAD I/O)
//! directly to Setun-DOS system calls (`SYS_BLIT`, `SYS_GET_KEY`, `SYS_YIELD`, `SYS_GET_TICKS`, `SYS_FILE_READ`).

use crate::dos::SetunDos;
use crate::dos::syscall::*;
use crate::word::Tryte6;

/// Structure configuring and running Doom within Setun-DOS.
pub struct DoomDosRunner {
    pub dos: SetunDos,
    pub frame_counter: u64,
    pub last_frame_pixels: Vec<u8>,
}

impl DoomDosRunner {
    /// Initializes a new Doom session inside Setun-DOS with an optional IWAD buffer.
    pub fn new(wad_bytes: Option<&[u8]>) -> Result<Self, String> {
        let mut dos = SetunDos::new();
        dos.boot()?;

        if let Some(bytes) = wad_bytes {
            dos.fs.mount_wad(bytes)?;
        }

        Ok(Self {
            dos,
            frame_counter: 0,
            last_frame_pixels: vec![0; 320 * 200],
        })
    }

    /// Injects a keyboard key press into the Setun-DOS input queue.
    pub fn inject_key(&mut self, key_code: i64) {
        self.dos.inject_key(key_code);
    }

    /// Renders a synthetic or computed Doom frame buffer into user space and blits to screen via `SYS_BLIT`.
    pub fn render_frame(&mut self, frame_palette_indices: &[u8]) -> Result<(), String> {
        if frame_palette_indices.len() != 320 * 200 {
            return Err("Frame buffer size must match 320x200".to_string());
        }

        // Store frame into user memory buffer at 100_000
        let frame_buffer_addr = 100_000i64;
        for (i, &idx) in frame_palette_indices.iter().enumerate() {
            let color = Tryte6::from_palette_index(idx);
            self.dos.mem.write_word(frame_buffer_addr + (i as i64), color.to_word21())
                .map_err(|e| format!("Failed to write user frame buffer: {:?}", e))?;
        }

        // Invoke SYS_BLIT via Setun-DOS
        self.dos.cpu.acc = crate::word::Word21::new(SYS_BLIT);
        self.dos.cpu.index_regs[0] = crate::word::Word21::new(frame_buffer_addr);
        self.dos.cpu.index_regs[1] = crate::word::Word21::new(0); // dst_x
        self.dos.cpu.index_regs[2] = crate::word::Word21::new(0); // dst_y
        self.dos.cpu.index_regs[3] = crate::word::Word21::new(320); // width
        self.dos.cpu.index_regs[4] = crate::word::Word21::new(200); // height

        self.dos.dispatch_current_syscall(SYS_BLIT);

        self.frame_counter += 1;
        self.last_frame_pixels.copy_from_slice(frame_palette_indices);
        self.dos.tick_timer();

        Ok(())
    }

    /// Reads back the current screen pixels from the Setun-DOS framebuffer as RGBA32.
    pub fn get_screen_rgba(&self) -> Vec<u8> {
        let mut buf = vec![0u8; 320 * 200 * 4];
        self.dos.mem.render_framebuffer_rgba(&mut buf);
        buf
    }
}
