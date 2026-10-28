//! Memory-Mapped I/O Subsystem (MMO / MMIO), DMA Controller / Binary File Loader
//! (`DOOM.WAD`), and Native C FFI Interface for `doomgeneric`.
//!
//! This module implements:
//! 1. **Ternary DMA / File Loader (`WadDmaLoader`)**: reads binary byte streams (e.g. `DOOM.WAD`)
//!    from disk or in-memory buffers and transcodes into balanced ternary words (`Word21`),
//!    supporting:
//!    - `TrytePerByte` mode (1 binary byte -> 1 6-trit `Tryte6` centered in `[-128, +127]`).
//!    - `PackedI32` mode (groups of 4 Little-Endian `i32` bytes -> 1 21-trit `Word21`,
//!      since $3^{21} > 2^{32}$).
//!    - WAD header parser (`IWAD`/`PWAD`) that locates the `PLAYPAL` lump and automatically
//!      updates VGA memory palette entries.
//! 2. **`doomgeneric` Platform Bridge**: exports the 5 required functions with C ABI (`extern "C"`):
//!    - `DG_Init`
//!    - `DG_DrawFrame`
//!    - `DG_SleepMs`
//!    - `DG_GetTicksMs`
//!    - `DG_GetKey`

use crate::cpu::Cpu;
use crate::memory::{
    Memory, MemoryError, FB_SIZE, MMIO_KEY_STATE, MMIO_TICKS_MS, MMIO_WAD_SIZE, WAD_DMA_BASE,
};
use crate::word::{Tryte6, Word21};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// Binary-to-ternary transcoding modes during DMA transfers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DmaTranscodeMode {
    /// Each `u8` byte (`0..=255`) is transcoded into a word containing a balanced `Tryte6`
    /// (`byte as i16 - 128` in `[-128, +127]`).
    TrytePerByte,
    /// Each 4-byte Little-Endian block (`i32`) is transcoded directly into a
    /// balanced ternary `Word21` (1 21-trit word per 32-bit integer).
    PackedI32LittleEndian,
}

/// Descriptor of a lump extracted from a WAD file directory (`IWAD` or `PWAD`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WadLumpInfo {
    /// ASCII name of up to 8 characters for lump (e.g. `"PLAYPAL"`, `"E1M1"`, `"VERTEXES"`).
    pub name: String,
    /// Byte offset in original WAD file.
    pub file_pos: usize,
    /// Byte size of lump.
    pub size: usize,
    /// Base address in ternary memory where lump was loaded (if mapped).
    pub ternary_addr: usize,
}

/// DMA controller and loader capable of reading binary files (`DOOM.WAD`) and transcoding
/// into balanced ternary words during initialization.
pub struct WadDmaLoader;

impl WadDmaLoader {
    /// Transcodes a slice of binary bytes into ternary memory starting at `base_addr`.
    /// Returns the count of `Word21` words written to memory.
    pub fn load_bytes(
        mem: &mut Memory,
        base_addr: usize,
        bytes: &[u8],
        mode: DmaTranscodeMode,
    ) -> Result<usize, MemoryError> {
        match mode {
            DmaTranscodeMode::TrytePerByte => {
                if base_addr + bytes.len() > mem.raw_cells().len() {
                    return Err(MemoryError::DmaOverflow {
                        base: base_addr,
                        len: bytes.len(),
                    });
                }
                for (i, &b) in bytes.iter().enumerate() {
                    let tryte = Tryte6::from_u8_centered(b);
                    mem.write_word_fast((base_addr + i) as i64, Word21::new(tryte.as_i64()));
                }
                mem.write_word_fast(MMIO_WAD_SIZE as i64, Word21::new(bytes.len() as i64));
                Ok(bytes.len())
            }
            DmaTranscodeMode::PackedI32LittleEndian => {
                let word_count = bytes.len().div_ceil(4);
                if base_addr + word_count > mem.raw_cells().len() {
                    return Err(MemoryError::DmaOverflow {
                        base: base_addr,
                        len: word_count,
                    });
                }
                for (chunk_idx, chunk) in bytes.chunks(4).enumerate() {
                    let mut buf = [0u8; 4];
                    buf[..chunk.len()].copy_from_slice(chunk);
                    let val_i32 = i32::from_le_bytes(buf);
                    mem.write_word_fast((base_addr + chunk_idx) as i64, Word21::from_i32(val_i32));
                }
                mem.write_word_fast(MMIO_WAD_SIZE as i64, Word21::new(word_count as i64));
                Ok(word_count)
            }
        }
    }

    /// Reconstructs binary bytes from ternary words loaded in `TrytePerByte` mode.
    pub fn read_bytes_tryte(
        mem: &Memory,
        base_addr: usize,
        len: usize,
    ) -> Result<Vec<u8>, MemoryError> {
        if base_addr + len > mem.raw_cells().len() {
            return Err(MemoryError::OutOfBounds((base_addr + len) as i64));
        }
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let w = mem.read_word_fast((base_addr + i) as i64);
            let tryte = Tryte6::from_i64(w.as_i64());
            out.push(tryte.to_u8_centered());
        }
        Ok(out)
    }

    /// Reads a binary file from disk (e.g. `DOOM.WAD` or `DOOM1.WAD`), transcodes its contents
    /// into ternary memory at `base_addr`, and if it contains a valid `PLAYPAL` lump,
    /// automatically initializes the 256-color palette in memory.
    pub fn load_wad_file<P: AsRef<Path>>(
        mem: &mut Memory,
        path: P,
        base_addr: usize,
        mode: DmaTranscodeMode,
    ) -> Result<(usize, Vec<WadLumpInfo>), String> {
        let bytes = fs::read(path.as_ref())
            .map_err(|e| format!("Failed to read WAD file {:?}: {}", path.as_ref(), e))?;
        Self::load_wad_from_slice(mem, &bytes, base_addr, mode)
    }

    /// Parses a WAD buffer (`IWAD`/`PWAD`), extracts its lump directory, loads `PLAYPAL` if
    /// present, and transcodes the entire binary image into ternary memory via DMA.
    pub fn load_wad_from_slice(
        mem: &mut Memory,
        bytes: &[u8],
        base_addr: usize,
        mode: DmaTranscodeMode,
    ) -> Result<(usize, Vec<WadLumpInfo>), String> {
        let words_written = Self::load_bytes(mem, base_addr, bytes, mode)
            .map_err(|e| format!("DMA error loading WAD: {:?}", e))?;

        let mut lumps = Vec::new();
        if bytes.len() >= 12 && (&bytes[0..4] == b"IWAD" || &bytes[0..4] == b"PWAD") {
            let num_lumps = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
            let info_table_ofs =
                i32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;

            if info_table_ofs + num_lumps.saturating_mul(16) <= bytes.len() {
                for i in 0..num_lumps {
                    let entry_ofs = info_table_ofs + i * 16;
                    let file_pos = i32::from_le_bytes([
                        bytes[entry_ofs],
                        bytes[entry_ofs + 1],
                        bytes[entry_ofs + 2],
                        bytes[entry_ofs + 3],
                    ]) as usize;
                    let size = i32::from_le_bytes([
                        bytes[entry_ofs + 4],
                        bytes[entry_ofs + 5],
                        bytes[entry_ofs + 6],
                        bytes[entry_ofs + 7],
                    ]) as usize;
                    let name_bytes = &bytes[entry_ofs + 8..entry_ofs + 16];
                    let name_len = name_bytes
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(name_bytes.len());
                    let name = String::from_utf8_lossy(&name_bytes[..name_len]).to_string();

                    if name == "PLAYPAL" && file_pos + 768 <= bytes.len() && size >= 768 {
                        mem.load_playpal(&bytes[file_pos..file_pos + 768]);
                    }

                    let ternary_addr = match mode {
                        DmaTranscodeMode::TrytePerByte => base_addr + file_pos,
                        DmaTranscodeMode::PackedI32LittleEndian => base_addr + (file_pos / 4),
                    };

                    lumps.push(WadLumpInfo {
                        name,
                        file_pos,
                        size,
                        ternary_addr,
                    });
                }
            }
        }

        Ok((words_written, lumps))
    }
}

/// Key event enqueued for consumption by `DG_GetKey`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// `true` if key pressed (`1`), `false` if released (`0`).
    pub pressed: bool,
    /// Key code compatible with `doomkeys.h`.
    pub doom_key: u8,
}

/// Entry point addresses for Ternary Firmware ROM resident in `SetunMemory`.
pub const ROM_FIXED_MUL_ENTRY: i64 = 100;
pub const ROM_FIXED_DIV_ENTRY: i64 = 120;
pub const ROM_POINT_ON_SIDE_ENTRY: i64 = 140;
pub const ROM_APROX_DIST_ENTRY: i64 = 220;

/// Integrated Virtual Machine (`SetunDoomMachine`) unifying CPU, Memory, RGBA Framebuffer,
/// Monotonic Clock, and Keyboard Event Queue for integration with `doomgeneric`.
pub struct SetunDoomMachine {
    pub cpu: Cpu,
    pub memory: Memory,
    pub start_instant: Instant,
    pub key_queue: VecDeque<KeyEvent>,
    /// RGBA buffer (`320 * 200 * 4` bytes) updated on each `DG_DrawFrame` call.
    pub rgba_frame: Vec<u8>,
    /// Rendered frames counter.
    pub frames_drawn: u64,
}

impl SetunDoomMachine {
    /// Creates and initializes full Setun-Doom machine and writes ternary Firmware ROM to memory.
    pub fn new() -> Self {
        let mut machine = Self {
            cpu: Cpu::new(),
            memory: Memory::new(),
            start_instant: Instant::now(),
            key_queue: VecDeque::with_capacity(64),
            rgba_frame: vec![0u8; FB_SIZE * 4],
            frames_drawn: 0,
        };
        machine.install_firmware_rom();
        machine
    }

    /// Assembles and loads 21-trit firmware subroutines into `SetunMemory`
    /// (`ROM_FIXED_MUL`, `ROM_FIXED_DIV`, `ROM_POINT_ON_SIDE`).
    fn install_firmware_rom(&mut self) {
        use crate::assembler::SetunAssembler;

        let mul_asm = r#"
            LOAD 50
            MUL 51
            STORE 52
            HALT
        "#;
        let mul_prog = SetunAssembler::assemble_at(mul_asm, ROM_FIXED_MUL_ENTRY)
            .expect("Failed to assemble ROM_FIXED_MUL during initialization");
        self.cpu
            .load_program(&mut self.memory, ROM_FIXED_MUL_ENTRY, &mul_prog);

        let div_asm = r#"
            LOAD 50
            DIV 51
            STORE 52
            HALT
        "#;
        let div_prog = SetunAssembler::assemble_at(div_asm, ROM_FIXED_DIV_ENTRY)
            .expect("Failed to assemble ROM_FIXED_DIV during initialization");
        self.cpu
            .load_program(&mut self.memory, ROM_FIXED_DIV_ENTRY, &div_prog);

        let pos_asm = r#"
            LOAD 64
            JMP_Z .ldx_is_zero
            LOAD 65
            JMP_Z .ldy_is_zero
            LOAD 60
            SUB 62
            STORE 66
            LOAD 61
            SUB 63
            STORE 67
            LOAD 65
            SHIFT -10
            STORE 68
            LOAD 64
            SHIFT -10
            MUL_ZERO 67
            LOAD_R 66
            MAC_NEG 68
            JMP_N .ret_zero
            JMP .ret_one
        .ldx_is_zero:
            LOAD 60
            CMP_T 62
            JMP_LE .ldx0_x_le_lx
            LOAD 65
            JMP_N .ret_one
            JMP .ret_zero
        .ldx0_x_le_lx:
            LOAD 65
            JMP_P .ret_one
            JMP .ret_zero
        .ldy_is_zero:
            LOAD 61
            CMP_T 63
            JMP_LE .ldy0_y_le_ly
            LOAD 64
            JMP_P .ret_one
            JMP .ret_zero
        .ldy0_y_le_ly:
            LOAD 64
            JMP_N .ret_one
            JMP .ret_zero
        .ret_one:
            LOAD_IMM 1
            STORE 69
            HALT
        .ret_zero:
            LOAD_IMM 0
            STORE 69
            HALT
        "#;
        let pos_prog = SetunAssembler::assemble_at(pos_asm, ROM_POINT_ON_SIDE_ENTRY)
            .expect("Failed to assemble ROM_POINT_ON_SIDE during initialization");
        self.cpu
            .load_program(&mut self.memory, ROM_POINT_ON_SIDE_ENTRY, &pos_prog);

        // Constant 0.5 in TernaryFixed (11.10) in MEM[73] for P_AproxDistance
        self.memory
            .write_word_fast(73, crate::fixed::TernaryFixed::from_f64(0.5).raw_word());

        let dist_asm = r#"
            LOAD 70
            JMP_GE .dx_pos
            STI.T
            STORE 70
        .dx_pos:
            LOAD 71
            JMP_GE .dy_pos
            STI.T
            STORE 71
        .dy_pos:
            LOAD 70
            ADD 71
            STORE 72
            LOAD 70
            CMP_T 71
            JMP_N .dx_smaller
            LOAD 71
            JMP .sub_half
        .dx_smaller:
            LOAD 70
        .sub_half:
            MUL 73
            STORE 74
            LOAD 72
            SUB 74
            STORE 75
            HALT
        "#;
        let dist_prog = SetunAssembler::assemble_at(dist_asm, ROM_APROX_DIST_ENTRY)
            .expect("Failed to assemble ROM_APROX_DIST during initialization");
        self.cpu
            .load_program(&mut self.memory, ROM_APROX_DIST_ENTRY, &dist_prog);
    }

    /// Executes a firmware ROM routine from `entry_pc` until reaching `HALT`,
    /// aborting immediately if any `CpuTrap` occurs or if execution exceeds `max_cycles`.
    #[inline]
    pub fn execute_rom_routine(&mut self, entry_pc: i64, max_cycles: usize) {
        self.cpu.pc = entry_pc;
        self.cpu.halted = false;
        self.cpu.trap = None;
        self.cpu.run_ticks(&mut self.memory, max_cycles);
        assert!(
            self.cpu.halted && self.cpu.trap.is_none(),
            "ROM routine at PC={} did not terminate with clean HALT (halted={}, trap={:?})",
            entry_pc,
            self.cpu.halted,
            self.cpu.trap
        );
    }

    /// Resets clock and platform registers (`DG_Init`).
    pub fn dg_init(&mut self) {
        self.start_instant = Instant::now();
        self.key_queue.clear();
        self.frames_drawn = 0;
        self.memory
            .write_word_fast(MMIO_TICKS_MS as i64, Word21::ZERO);
        self.memory
            .write_word_fast(MMIO_KEY_STATE as i64, Word21::ZERO);
    }

    /// Reads 64,000 `Tryte6` entries from ternary framebuffer and converts to
    /// RGBA buffer (`320x200x4` bytes) consumable by host display window (`DG_DrawFrame`).
    pub fn dg_draw_frame(&mut self) -> &[u8] {
        self.memory.render_framebuffer_rgba(&mut self.rgba_frame);
        self.frames_drawn += 1;
        &self.rgba_frame
    }

    /// Suspends current thread for `ms` milliseconds (`DG_SleepMs`).
    pub fn dg_sleep_ms(&self, ms: u32) {
        if ms > 0 {
            thread::sleep(Duration::from_millis(ms as u64));
        }
    }

    /// Returns elapsed time since `dg_init` in milliseconds and updates memory-mapped
    /// register `MMIO_TICKS_MS` (`DG_GetTicksMs`).
    pub fn dg_get_ticks_ms(&mut self) -> u32 {
        let elapsed = self.start_instant.elapsed().as_millis() as u32;
        self.memory
            .write_word_fast(MMIO_TICKS_MS as i64, Word21::new(elapsed as i64));
        elapsed
    }

    /// Enqueues a key event into keyboard controller and reflects state in MMIO.
    pub fn push_key_event(&mut self, pressed: bool, doom_key: u8) {
        self.key_queue.push_back(KeyEvent { pressed, doom_key });
        // Encodes state into MMIO_KEY_STATE: sign (+1 = pressed, -1 = released) * (doom_key + 1)
        let sign = if pressed { 1i64 } else { -1i64 };
        let encoded = sign * ((doom_key as i64) + 1);
        self.memory
            .write_word_fast(MMIO_KEY_STATE as i64, Word21::new(encoded));
    }

    /// Dequeues next keyboard event (`DG_GetKey`).
    /// Returns `Some((pressed, doom_key))` if key pending, or `None` if queue empty.
    pub fn dg_get_key(&mut self) -> Option<(i32, u8)> {
        if let Some(ev) = self.key_queue.pop_front() {
            Some((if ev.pressed { 1 } else { 0 }, ev.doom_key))
        } else {
            self.memory
                .write_word_fast(MMIO_KEY_STATE as i64, Word21::ZERO);
            None
        }
    }

    /// Loads a WAD buffer into default `WAD_DMA_BASE` region via DMA.
    pub fn load_wad_bytes(&mut self, bytes: &[u8]) -> Result<(usize, Vec<WadLumpInfo>), String> {
        WadDmaLoader::load_wad_from_slice(
            &mut self.memory,
            bytes,
            WAD_DMA_BASE,
            DmaTranscodeMode::TrytePerByte,
        )
    }

    /// Loads an instruction slice into ternary memory positioning `PC` at `base_addr`.
    pub fn load_program(&mut self, base_addr: i64, instructions: &[crate::cpu::Instruction]) {
        self.cpu
            .load_program(&mut self.memory, base_addr, instructions);
    }

    /// Executes single CPU step (`step()`).
    #[inline(always)]
    pub fn step(&mut self) -> bool {
        self.cpu.step(&mut self.memory)
    }

    /// Executes up to `count` CPU cycles (`run_ticks(count)`).
    pub fn run_ticks(&mut self, count: usize) -> usize {
        self.cpu.run_ticks(&mut self.memory, count)
    }
}

impl Default for SetunDoomMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Global thread-safe instance for direct servicing of `doomgeneric` C FFI calls.
static GLOBAL_MACHINE: OnceLock<Mutex<SetunDoomMachine>> = OnceLock::new();

/// Returns a reference to the global Setun-Doom machine mutex.
pub fn global_machine() -> &'static Mutex<SetunDoomMachine> {
    GLOBAL_MACHINE.get_or_init(|| Mutex::new(SetunDoomMachine::new()))
}

// ============================================================================
// C FFI bindings for direct integration with `doomgeneric`
// ============================================================================

/// Initializes Setun-Doom platform for `doomgeneric` main loop.
#[no_mangle]
pub extern "C" fn DG_Init() {
    if let Ok(mut machine) = global_machine().lock() {
        machine.dg_init();
    }
}

/// Translates 64,000 `Tryte6` entries from ternary framebuffer to RGBA window buffer.
/// If optional pointer `out_rgba_buffer` (`*mut u8` of 256,000 bytes) is provided by host,
/// also copies RGBA pixels to external buffer.
#[no_mangle]
pub extern "C" fn DG_DrawFrame() {
    if let Ok(mut machine) = global_machine().lock() {
        let _ = machine.dg_draw_frame();
    }
}

/// Extended FFI variant copying rendered RGBA framebuffer directly to C buffer
/// `out_rgba` of `320 * 200 * 4 = 256,000` bytes.
///
/// # Safety
/// `out_rgba` must be null or point to at least 256,000 valid writable bytes.
#[no_mangle]
pub unsafe extern "C" fn DG_DrawFrameToBuffer(out_rgba: *mut u8) {
    if let Ok(mut machine) = global_machine().lock() {
        let rgba = machine.dg_draw_frame();
        if !out_rgba.is_null() {
            core::ptr::copy_nonoverlapping(rgba.as_ptr(), out_rgba, FB_SIZE * 4);
        }
    }
}

/// Suspends execution for `ms` milliseconds.
#[no_mangle]
pub extern "C" fn DG_SleepMs(ms: u32) {
    if let Ok(machine) = global_machine().lock() {
        machine.dg_sleep_ms(ms);
    } else if ms > 0 {
        thread::sleep(Duration::from_millis(ms as u64));
    }
}

/// Returns millisecond count elapsed since `DG_Init`.
#[no_mangle]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    if let Ok(mut machine) = global_machine().lock() {
        machine.dg_get_ticks_ms()
    } else {
        0
    }
}

/// Queries `doomgeneric` keyboard event queue.
/// Populates `*pressed` (`1` or `0`) and `*doom_key` returning `1` if event was queued, or `0` otherwise.
///
/// # Safety
/// `pressed` and `doom_key` must be valid aligned pointers when events are present in queue.
#[no_mangle]
pub unsafe extern "C" fn DG_GetKey(pressed: *mut i32, doom_key: *mut u8) -> i32 {
    if pressed.is_null() || doom_key.is_null() {
        return 0;
    }
    if let Ok(mut machine) = global_machine().lock() {
        if let Some((is_pressed, key)) = machine.dg_get_key() {
            *pressed = is_pressed;
            *doom_key = key;
            return 1;
        }
    }
    0
}

/// Sets graphics window title (`DG_SetWindowTitle` from `doomgeneric`).
#[no_mangle]
pub extern "C" fn DG_SetWindowTitle(_title: *const core::ffi::c_char) {}

/// Enqueues key event from SDL2/X11 window directly into `MMIO_KEY_STATE` register
/// and keyboard controller queue of Setun-Doom machine.
#[no_mangle]
pub extern "C" fn setun_PushKeyEvent(pressed: i32, doom_key: u8) {
    if let Ok(mut machine) = global_machine().lock() {
        machine.push_key_event(pressed != 0, doom_key);
    }
}

// ============================================================================
// Ternary Arithmetic and Framebuffer Bridge for Doom C engine
// ============================================================================

/// Replaces `FixedMul(fixed_t a, fixed_t b)` in Doom `m_fixed.c`:
/// Writes operands into `MEM[50..51]`, executes ternary program `ROM_FIXED_MUL_ENTRY`
/// via `Cpu::step_checked()` (full Fetch-Decode-Execute cycle using physical 42-trit multiplier
/// and unbiased $3^{10}$ vs $2^{16}$ telemetry), and reads `MEM[52]`.
#[no_mangle]
pub extern "C" fn setun_FixedMul(a: i32, b: i32) -> i32 {
    let ta = crate::fixed::TernaryFixed::from_doom_fixed16(a);
    let tb = crate::fixed::TernaryFixed::from_doom_fixed16(b);
    let mut machine = global_machine()
        .lock()
        .expect("Corrupted SetunDoomMachine mutex in setun_FixedMul");
    machine.memory.write_word_fast(50, ta.raw_word());
    machine.memory.write_word_fast(51, tb.raw_word());
    machine.execute_rom_routine(ROM_FIXED_MUL_ENTRY, 8);
    let prod_word = machine.memory.read_word_fast(52);
    crate::fixed::TernaryFixed::from_raw(prod_word).to_doom_fixed16()
}

/// Replaces `FixedDiv(fixed_t a, fixed_t b)` in Doom `m_fixed.c`:
/// Writes operands into `MEM[50..51]` and executes ternary program `ROM_FIXED_DIV_ENTRY`
/// via `Cpu::step_checked()`.
#[no_mangle]
pub extern "C" fn setun_FixedDiv(a: i32, b: i32) -> i32 {
    if (a.unsigned_abs() >> 14) >= b.unsigned_abs() {
        return if (a ^ b) < 0 { i32::MIN } else { i32::MAX };
    }
    let ta = crate::fixed::TernaryFixed::from_doom_fixed16(a);
    let tb = crate::fixed::TernaryFixed::from_doom_fixed16(b);
    let mut machine = global_machine()
        .lock()
        .expect("Corrupted SetunDoomMachine mutex in setun_FixedDiv");
    machine.memory.write_word_fast(50, ta.raw_word());
    machine.memory.write_word_fast(51, tb.raw_word());
    machine.execute_rom_routine(ROM_FIXED_DIV_ENTRY, 8);
    let quot_word = machine.memory.read_word_fast(52);
    crate::fixed::TernaryFixed::from_raw(quot_word).to_doom_fixed16()
}

/// Executes Doom `R_PointOnSide` / `R_PointOnSegSide` / `P_PointOnLineSide`
/// entirely inside Setun-Doom CPU via `ROM_POINT_ON_SIDE_ENTRY` (`PC=140`),
/// maintaining all intermediate values (`dx`, `dy`, `left`, `right`) in `TernaryFixed` (`11.10`)
/// without intermediate `16.16 <-> 11.10` double quantization.
#[no_mangle]
pub extern "C" fn setun_PointOnSide(x: i32, y: i32, lx: i32, ly: i32, ldx: i32, ldy: i32) -> i32 {
    let tx = crate::fixed::TernaryFixed::from_doom_fixed16(x).raw_word();
    let ty = crate::fixed::TernaryFixed::from_doom_fixed16(y).raw_word();
    let tlx = crate::fixed::TernaryFixed::from_doom_fixed16(lx).raw_word();
    let tly = crate::fixed::TernaryFixed::from_doom_fixed16(ly).raw_word();
    let tldx = crate::fixed::TernaryFixed::from_doom_fixed16(ldx).raw_word();
    let tldy = crate::fixed::TernaryFixed::from_doom_fixed16(ldy).raw_word();

    let mut machine = global_machine()
        .lock()
        .expect("Corrupted SetunDoomMachine mutex in setun_PointOnSide");
    machine.memory.write_word_fast(60, tx);
    machine.memory.write_word_fast(61, ty);
    machine.memory.write_word_fast(62, tlx);
    machine.memory.write_word_fast(63, tly);
    machine.memory.write_word_fast(64, tldx);
    machine.memory.write_word_fast(65, tldy);
    machine.execute_rom_routine(ROM_POINT_ON_SIDE_ENTRY, 32);
    machine.memory.read_word_fast(69).as_i64() as i32
}

/// Executes Doom `P_AproxDistance(dx, dy)` in `p_maputl.c` entirely inside Setun-Doom CPU
/// via `ROM_APROX_DIST_ENTRY` (`PC=220`), using `STI.T` for ternary absolute value,
/// `CMP_T` for minimum and `ADD`/`SUB`/`MUL` in `TernaryFixed` (`11.10`).
#[no_mangle]
pub extern "C" fn setun_AproxDistance(dx: i32, dy: i32) -> i32 {
    let tdx = crate::fixed::TernaryFixed::from_doom_fixed16(dx).raw_word();
    let tdy = crate::fixed::TernaryFixed::from_doom_fixed16(dy).raw_word();
    let mut machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_AproxDistance");
    machine.memory.write_word_fast(70, tdx);
    machine.memory.write_word_fast(71, tdy);
    machine.execute_rom_routine(ROM_APROX_DIST_ENTRY, 32);
    let res_word = machine.memory.read_word_fast(75);
    crate::fixed::TernaryFixed::from_raw(res_word).to_doom_fixed16()
}

/// Executes column DDA interpolator `R_DrawColumn` from `r_draw.c`
/// using `TernaryFixed` accumulator (`11.10` trits, scale $3^{10} = 59,049$) and 21-stage
/// ripple-carry adder (`Word21::gate_ripple_carry_add`) per vertical pixel.
///
/// # Safety
/// `dest`, `dc_source`, and `dc_colormap` must be valid pointers to Doom framebuffer and lumps.
#[no_mangle]
#[allow(clippy::explicit_counter_loop)]
pub unsafe extern "C" fn setun_DrawColumn(
    dc_x: i32,
    dc_yl: i32,
    dc_yh: i32,
    centery: i32,
    dc_iscale: i32,
    dc_texturemid: i32,
    dc_source: *const u8,
    dc_colormap: *const u8,
    dest_start: *mut u8,
    screenwidth: i32,
) {
    let count = dc_yh - dc_yl;
    if count < 0 {
        return;
    }
    assert!(
        !dc_source.is_null() && !dc_colormap.is_null() && !dest_start.is_null(),
        "Null pointer in setun_DrawColumn"
    );
    assert!(
        dc_x >= 0
            && (dc_x as usize) < crate::memory::FB_WIDTH
            && dc_yl >= 0
            && (dc_yh as usize) < crate::memory::FB_HEIGHT,
        "Coordinates out of range in setun_DrawColumn: x={}, yl={}, yh={}",
        dc_x,
        dc_yl,
        dc_yh
    );

    // Initial step and coordinate in TernaryFixed (11.10 trits, scale 3^10 = 59049)
    let frac16_init = dc_texturemid.wrapping_add((dc_yl - centery).wrapping_mul(dc_iscale));
    let mut t_frac = crate::fixed::TernaryFixed::from_doom_fixed16(frac16_init).raw_word();
    let t_step = crate::fixed::TernaryFixed::from_doom_fixed16(dc_iscale).raw_word();

    let mut b_frac16 = frac16_init;
    let b_step16 = dc_iscale;

    let mut machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_DrawColumn");

    let mut dest_ptr = dest_start;
    let mut y = dc_yl as usize;
    let x = dc_x as usize;

    for _ in 0..=count {
        // Extracts texture line by shifting 10 trits physically across wires
        let int_row = t_frac.shift_trits(-10).as_i64();
        let tex_row = (int_row.rem_euclid(128)) as usize;
        let raw_texel = *dc_source.add(tex_row) as usize;
        let lit_color = *dc_colormap.add(raw_texel);

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        // Steps frac += fracstep using 21-gate `Trit::full_add` ripple-carry adder
        let (next_t_frac, _cout, carries) =
            t_frac.gate_ripple_carry_add(t_step, crate::trit::Trit::Zero);
        let next_b_frac16 = b_frac16.wrapping_add(b_step16);

        machine.cpu.telemetry.adder_carry_propagations += carries as u64;
        machine.cpu.telemetry.record_fixed16_transition(
            t_frac,
            next_t_frac,
            b_frac16,
            next_b_frac16,
        );
        machine.cpu.cycles += 1;

        t_frac = next_t_frac;
        b_frac16 = next_b_frac16;
        dest_ptr = dest_ptr.add(screenwidth as usize);
        y += 1;
    }
    machine.cpu.acc = t_frac;
}

/// Executes color-translated column interpolator `R_DrawTranslatedColumn` from `r_draw.c`
/// (used for player sprites and monster variants) using `TernaryFixed` accumulator
/// and 21-gate `Trit::full_add` ripple-carry adder.
///
/// # Safety
/// Pointers must be valid for reading and writing in framebuffer.
#[no_mangle]
#[allow(clippy::explicit_counter_loop)]
pub unsafe extern "C" fn setun_DrawTranslatedColumn(
    dc_x: i32,
    dc_yl: i32,
    dc_yh: i32,
    centery: i32,
    dc_iscale: i32,
    dc_texturemid: i32,
    dc_source: *const u8,
    dc_colormap: *const u8,
    dc_translation: *const u8,
    dest_start: *mut u8,
    screenwidth: i32,
) {
    let count = dc_yh - dc_yl;
    if count < 0 {
        return;
    }
    assert!(
        !dc_source.is_null()
            && !dc_colormap.is_null()
            && !dc_translation.is_null()
            && !dest_start.is_null(),
        "Null pointer in setun_DrawTranslatedColumn"
    );

    let frac16_init = dc_texturemid.wrapping_add((dc_yl - centery).wrapping_mul(dc_iscale));
    let mut t_frac = crate::fixed::TernaryFixed::from_doom_fixed16(frac16_init).raw_word();
    let t_step = crate::fixed::TernaryFixed::from_doom_fixed16(dc_iscale).raw_word();

    let mut b_frac16 = frac16_init;
    let b_step16 = dc_iscale;

    let mut machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_DrawTranslatedColumn");

    let mut dest_ptr = dest_start;
    let mut y = dc_yl as usize;
    let x = dc_x as usize;

    for _ in 0..=count {
        let int_row = t_frac.shift_trits(-10).as_i64();
        let tex_row = (int_row.rem_euclid(128)) as usize;
        let raw_texel = *dc_source.add(tex_row) as usize;
        let trans_texel = *dc_translation.add(raw_texel) as usize;
        let lit_color = *dc_colormap.add(trans_texel);

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        let (next_t_frac, _cout, carries) =
            t_frac.gate_ripple_carry_add(t_step, crate::trit::Trit::Zero);
        let next_b_frac16 = b_frac16.wrapping_add(b_step16);

        machine.cpu.telemetry.adder_carry_propagations += carries as u64;
        machine.cpu.telemetry.record_fixed16_transition(
            t_frac,
            next_t_frac,
            b_frac16,
            next_b_frac16,
        );
        machine.cpu.cycles += 1;

        t_frac = next_t_frac;
        b_frac16 = next_b_frac16;
        dest_ptr = dest_ptr.add(screenwidth as usize);
        y += 1;
    }
    machine.cpu.acc = t_frac;
}

/// Executes fuzz/invisibility column rasterizer `R_DrawFuzzColumn` from `r_draw.c`
/// using 21-gate `Trit::full_add` ripple-carry adder to advance `frac` per pixel.
///
/// # Safety
/// Pointers must be valid.
#[no_mangle]
#[allow(clippy::explicit_counter_loop)]
pub unsafe extern "C" fn setun_DrawFuzzColumn(
    dc_x: i32,
    dc_yl: i32,
    dc_yh: i32,
    centery: i32,
    dc_iscale: i32,
    dc_texturemid: i32,
    colormaps: *const u8,
    fuzzoffset: *const i32,
    fuzzpos_ptr: *mut i32,
    fuzztable_len: i32,
    dest_start: *mut u8,
    screenwidth: i32,
) {
    let count = dc_yh - dc_yl;
    if count < 0 {
        return;
    }
    assert!(
        !colormaps.is_null()
            && !fuzzoffset.is_null()
            && !fuzzpos_ptr.is_null()
            && !dest_start.is_null(),
        "Null pointer in setun_DrawFuzzColumn"
    );

    let frac16_init = dc_texturemid.wrapping_add((dc_yl - centery).wrapping_mul(dc_iscale));
    let mut t_frac = crate::fixed::TernaryFixed::from_doom_fixed16(frac16_init).raw_word();
    let t_step = crate::fixed::TernaryFixed::from_doom_fixed16(dc_iscale).raw_word();

    let mut b_frac16 = frac16_init;
    let b_step16 = dc_iscale;

    let mut machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_DrawFuzzColumn");

    let mut dest_ptr = dest_start;
    let mut y = dc_yl as usize;
    let x = dc_x as usize;
    let mut fuzzpos = *fuzzpos_ptr;

    for _ in 0..=count {
        let f_off = *fuzzoffset.add(fuzzpos as usize);
        let sampled_pixel = *dest_ptr.offset(f_off as isize);
        let lit_color = *colormaps.add(6 * 256 + (sampled_pixel as usize));

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        fuzzpos += 1;
        if fuzzpos >= fuzztable_len {
            fuzzpos = 0;
        }

        let (next_t_frac, _cout, carries) =
            t_frac.gate_ripple_carry_add(t_step, crate::trit::Trit::Zero);
        let next_b_frac16 = b_frac16.wrapping_add(b_step16);

        machine.cpu.telemetry.adder_carry_propagations += carries as u64;
        machine.cpu.telemetry.record_fixed16_transition(
            t_frac,
            next_t_frac,
            b_frac16,
            next_b_frac16,
        );
        machine.cpu.cycles += 1;

        t_frac = next_t_frac;
        b_frac16 = next_b_frac16;
        dest_ptr = dest_ptr.add(screenwidth as usize);
        y += 1;
    }
    *fuzzpos_ptr = fuzzpos;
    machine.cpu.acc = t_frac;
}

/// Executes horizontal span (floors and ceilings) DDA interpolator `R_DrawSpan` from `r_draw.c`
/// using `TernaryFixed` accumulators (`11.10` trits) and 21-stage ripple-carry adder
/// (`Word21::gate_ripple_carry_add`) for `(u, v)` coordinates per pixel.
///
/// # Safety
/// `dest_start`, `ds_source`, and `ds_colormap` must be valid pointers.
#[no_mangle]
#[allow(clippy::explicit_counter_loop)]
pub unsafe extern "C" fn setun_DrawSpan(
    ds_y: i32,
    ds_x1: i32,
    ds_x2: i32,
    ds_xfrac: i32,
    ds_yfrac: i32,
    ds_xstep: i32,
    ds_ystep: i32,
    ds_source: *const u8,
    ds_colormap: *const u8,
    dest_start: *mut u8,
) {
    let count = ds_x2 - ds_x1;
    if count < 0 {
        return;
    }
    assert!(
        !ds_source.is_null() && !ds_colormap.is_null() && !dest_start.is_null(),
        "Null pointer in setun_DrawSpan"
    );
    assert!(
        ds_x1 >= 0
            && (ds_x2 as usize) < crate::memory::FB_WIDTH
            && ds_y >= 0
            && (ds_y as usize) < crate::memory::FB_HEIGHT,
        "Coordinates out of range in setun_DrawSpan: y={}, x1={}, x2={}",
        ds_y,
        ds_x1,
        ds_x2
    );

    let mut tx_frac = crate::fixed::TernaryFixed::from_doom_fixed16(ds_xfrac).raw_word();
    let mut ty_frac = crate::fixed::TernaryFixed::from_doom_fixed16(ds_yfrac).raw_word();
    let tx_step = crate::fixed::TernaryFixed::from_doom_fixed16(ds_xstep).raw_word();
    let ty_step = crate::fixed::TernaryFixed::from_doom_fixed16(ds_ystep).raw_word();

    let mut bx_frac16 = ds_xfrac;
    let mut by_frac16 = ds_yfrac;

    let mut machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_DrawSpan");

    let mut dest_ptr = dest_start;
    let y = ds_y as usize;
    let mut x = ds_x1 as usize;

    for _ in 0..=count {
        // Extracts integer texture coordinates (u, v) by shifting 10 trits physically across wires
        let u_int = (tx_frac.shift_trits(-10).as_i64().rem_euclid(64)) as usize;
        let v_int = (ty_frac.shift_trits(-10).as_i64().rem_euclid(64)) as usize;
        let spot = (v_int * 64) + u_int;
        let raw_texel = *ds_source.add(spot) as usize;
        let lit_color = *ds_colormap.add(raw_texel);

        *dest_ptr = lit_color;
        let fb_offset = y * crate::memory::FB_WIDTH + x;
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + fb_offset] =
            Tryte6::from_palette_index(lit_color).as_i64();

        let (next_tx, _c1, carries_x) =
            tx_frac.gate_ripple_carry_add(tx_step, crate::trit::Trit::Zero);
        let (next_ty, _c2, carries_y) =
            ty_frac.gate_ripple_carry_add(ty_step, crate::trit::Trit::Zero);
        let next_bx16 = bx_frac16.wrapping_add(ds_xstep);
        let next_by16 = by_frac16.wrapping_add(ds_ystep);

        machine.cpu.telemetry.adder_carry_propagations += (carries_x + carries_y) as u64;
        machine
            .cpu
            .telemetry
            .record_fixed16_transition(tx_frac, next_tx, bx_frac16, next_bx16);
        machine
            .cpu
            .telemetry
            .record_fixed16_transition(ty_frac, next_ty, by_frac16, next_by16);
        machine.cpu.cycles += 2;

        tx_frac = next_tx;
        ty_frac = next_ty;
        bx_frac16 = next_bx16;
        by_frac16 = next_by16;
        dest_ptr = dest_ptr.add(1);
        x += 1;
    }
    machine.cpu.acc = tx_frac;
}

static ACTIVE_PALETTE: Mutex<Option<[u8; 768]>> = Mutex::new(None);

/// Synchronizes 256-color RGB `PLAYPAL` palette from `I_SetPalette` in `doomgeneric`
/// with the `PALETTE_BASE` region of Setun-Doom ternary memory.
///
/// # Safety
/// `rgb_palette` must point to 768 bytes (`256 * 3`).
#[no_mangle]
pub unsafe extern "C" fn setun_SyncPalette(rgb_palette: *const u8) {
    assert!(
        !rgb_palette.is_null(),
        "Null pointer passed to setun_SyncPalette"
    );
    let slice = core::slice::from_raw_parts(rgb_palette, 768);
    if let Ok(mut lock) = ACTIVE_PALETTE.lock() {
        let mut arr = [0u8; 768];
        arr.copy_from_slice(slice);
        *lock = Some(arr);
    }
    let mut machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_SyncPalette");
    machine.memory.load_playpal(slice);
}

/// Retrieves the latest 768-byte palette synchronized by the Doom engine.
pub fn get_active_palette() -> Option<[u8; 768]> {
    ACTIVE_PALETTE.lock().ok().and_then(|p| *p)
}

/// Copies 64,000 bytes from `I_VideoBuffer` (`320x200` Doom palette indices)
/// to 64,000 `Tryte6` mapped at `FRAMEBUFFER_BASE` (`4_600_000..4_664_000`) in
/// Setun-Doom ternary memory and renders output RGBA32 buffer for display.
///
/// # Safety
/// `video_buffer_320x200` must point to 64,000 bytes and `out_rgba_320x200` to 256,000 bytes.
#[no_mangle]
pub unsafe extern "C" fn setun_BlitVideoBuffer(
    video_buffer_320x200: *const u8,
    out_rgba_320x200: *mut u8,
) {
    assert!(
        !video_buffer_320x200.is_null(),
        "Null pointer in setun_BlitVideoBuffer"
    );
    let vbuf = core::slice::from_raw_parts(video_buffer_320x200, FB_SIZE);
    let mut machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_BlitVideoBuffer");
    for (i, &pal_idx) in vbuf.iter().enumerate() {
        let tryte = Tryte6::from_palette_index(pal_idx);
        machine.memory.raw_cells_mut()[crate::memory::FRAMEBUFFER_BASE + i] = tryte.as_i64();
    }
    let rgba = machine.dg_draw_frame();
    if !out_rgba_320x200.is_null() {
        core::ptr::copy_nonoverlapping(rgba.as_ptr(), out_rgba_320x200, FB_SIZE * 4);
    }
}

/// Structure exported via C ABI containing register state and telemetry for HUD overlay.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct SetunLiveTelemetrySnapshot {
    pub acc_val: i64,
    pub r_val: i64,
    pub pc: i64,
    pub cycles: u64,
    pub adder_carry_propagations: u64,
    pub ternary_trit_flips: u64,
    pub ternary_half_swings: u64,
    pub ternary_full_swings: u64,
    pub binary32_bit_flips: u64,
    pub digit_reduction_pct: f32,
    pub energy_reduction_pct: f32,
    pub acc_trits: [i8; 21],
}

/// Collects an atomic live snapshot of register state and telemetry for HUD overlay.
///
/// # Safety
/// `out_snap` must point to valid aligned memory for [`SetunLiveTelemetrySnapshot`].
#[no_mangle]
pub unsafe extern "C" fn setun_GetLiveTelemetrySnapshot(out_snap: *mut SetunLiveTelemetrySnapshot) {
    if out_snap.is_null() {
        return;
    }
    let machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_GetLiveTelemetrySnapshot");
    let tel = machine.cpu.telemetry;
    let acc_word = machine.cpu.acc;
    let mut trits_arr = [0i8; 21];
    for (i, t) in acc_word.to_trits().iter().enumerate() {
        trits_arr[i] = t.as_i8();
    }

    *out_snap = SetunLiveTelemetrySnapshot {
        acc_val: acc_word.as_i64(),
        r_val: machine.cpu.r.as_i64(),
        pc: machine.cpu.pc,
        cycles: machine.cpu.cycles,
        adder_carry_propagations: tel.adder_carry_propagations,
        ternary_trit_flips: tel.ternary_trit_flips,
        ternary_half_swings: tel.ternary_half_swings,
        ternary_full_swings: tel.ternary_full_swings,
        binary32_bit_flips: tel.binary32_bit_flips,
        digit_reduction_pct: tel.digit_flip_reduction_pct() as f32,
        energy_reduction_pct: tel.weighted_energy_reduction_pct() as f32,
        acc_trits: trits_arr,
    };
}

/// Prints a live summary of accumulated ternary telemetry to terminal
/// during `doomgeneric` execution.
#[no_mangle]
pub extern "C" fn setun_PrintTelemetry(frame_num: u32) {
    let machine = global_machine()
        .lock()
        .expect("Corrupted mutex in setun_PrintTelemetry");
    let tel = machine.cpu.telemetry;
    println!(
        "[Setun-Doom Frame {:>4}] ISA Instrs: {:>8} | Gate Carries: {:>8} | Trit Flips: {:>9} (Half: {:>9}, Full: {:>7}) vs RV32I Bit Flips: {:>9} ({:+.2}% Flips | {:+.2}% Energy dV^2)",
        frame_num,
        machine.cpu.cycles,
        tel.adder_carry_propagations,
        tel.ternary_trit_flips,
        tel.ternary_half_swings,
        tel.ternary_full_swings,
        tel.binary32_bit_flips,
        -tel.digit_flip_reduction_pct(),
        -tel.weighted_energy_reduction_pct()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::FRAMEBUFFER_BASE;

    #[test]
    fn test_wad_dma_transcode_and_playpal_extraction() {
        let mut mem = Memory::new();

        // Constructs synthetic IWAD image with 1 lump named "PLAYPAL" of 768 bytes
        let mut wad = Vec::new();
        wad.extend_from_slice(b"IWAD");
        wad.extend_from_slice(&(1i32.to_le_bytes())); // 1 lump
        wad.extend_from_slice(&(28i32.to_le_bytes()));

        // Directory offset: 12 + 768 = 780
        wad.truncate(8);
        wad.extend_from_slice(&(780i32.to_le_bytes()));

        // PLAYPAL data (768 bytes = 256 * 3)
        for i in 0..256 {
            wad.push(i as u8); // R
            wad.push((255 - i) as u8); // G
            wad.push(128u8); // B
        }
        assert_eq!(wad.len(), 780);

        // Directory entry (16 bytes): filepos=12, size=768, name="PLAYPAL\0"
        wad.extend_from_slice(&(12i32.to_le_bytes()));
        wad.extend_from_slice(&(768i32.to_le_bytes()));
        wad.extend_from_slice(b"PLAYPAL\0");

        let (words, lumps) = WadDmaLoader::load_wad_from_slice(
            &mut mem,
            &wad,
            WAD_DMA_BASE,
            DmaTranscodeMode::TrytePerByte,
        )
        .unwrap();

        assert_eq!(words, wad.len());
        assert_eq!(lumps.len(), 1);
        assert_eq!(lumps[0].name, "PLAYPAL");

        // Verifies bytes can be recovered from ternary memory
        let recovered = WadDmaLoader::read_bytes_tryte(&mem, WAD_DMA_BASE, wad.len()).unwrap();
        assert_eq!(recovered, wad);
    }

    #[test]
    fn test_doomgeneric_ffi_functions() {
        DG_Init();
        DG_SleepMs(2);
        let ticks = DG_GetTicksMs();
        assert!(ticks >= 1);

        // Writes to global machine framebuffer and tests DG_DrawFrameToBuffer
        {
            let mut machine = global_machine().lock().unwrap();
            machine.memory.set_palette_entry(42, 10, 200, 30);
            let tryte = Tryte6::from_palette_index(42);
            machine
                .memory
                .write_word(FRAMEBUFFER_BASE as i64, Word21::new(tryte.as_i64()))
                .unwrap();
            machine.push_key_event(true, 0xad); // KEY_UPARROW
        }

        let mut out_rgba = vec![0u8; FB_SIZE * 4];
        // SAFETY: out_rgba is allocated with FB_SIZE * 4 bytes.
        unsafe {
            DG_DrawFrameToBuffer(out_rgba.as_mut_ptr());
        }
        assert_eq!(&out_rgba[0..4], &[10, 200, 30, 255]);

        let mut pressed = 0i32;
        let mut key = 0u8;
        // SAFETY: pressed and key are valid stack pointers.
        let has_key = unsafe { DG_GetKey(&mut pressed, &mut key) };
        assert_eq!(has_key, 1);
        assert_eq!(pressed, 1);
        assert_eq!(key, 0xad);

        // SAFETY: pressed and key are valid stack pointers.
        let empty_key = unsafe { DG_GetKey(&mut pressed, &mut key) };
        assert_eq!(empty_key, 0);
    }
}
