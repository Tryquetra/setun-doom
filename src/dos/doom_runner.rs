//! Doom execution harness running inside the Setun-DOS operating environment.
//!
//! Bridges Doom's `doomgeneric` contract (`DG_DrawFrame`, `DG_GetKey`, `DG_SleepMs`, `DG_GetTicksMs`, WAD I/O)
//! directly to Setun-DOS system calls (`SYS_BLIT`, `SYS_GET_KEY`, `SYS_YIELD`, `SYS_GET_TICKS`, `SYS_FILE_READ`).

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::dos::syscall::SYS_BLIT;
use crate::dos::SetunDos;
use crate::word::Tryte6;

static DOOM_INITIALIZED: AtomicBool = AtomicBool::new(false);
static DOOM_GLOBAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

extern "C" {
    fn doomgeneric_Create(argc: i32, argv: *mut *mut std::ffi::c_char);
    fn doomgeneric_Tick();
    static I_VideoBuffer: *const u8;
}

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
            dos.mount_wad(bytes)?;
        }

        Ok(Self {
            dos,
            frame_counter: 0,
            last_frame_pixels: vec![0; 320 * 200],
        })
    }

    /// Checks if the native Doom engine has already been initialized in this process.
    pub fn is_engine_initialized() -> bool {
        DOOM_INITIALIZED.load(Ordering::SeqCst)
    }

    /// Initializes the authentic 1993 Doom C engine with the specified WAD path.
    pub fn init_native_engine(&mut self, wad_path: &str) -> Result<(), String> {
        let _lock = DOOM_GLOBAL_LOCK
            .lock()
            .map_err(|e| format!("Failed to acquire Doom lock: {}", e))?;

        if !DOOM_INITIALIZED.load(Ordering::SeqCst) {
            // Leak arguments to static memory so C engine's global myargv remains valid permanently
            let args: &'static [CString] = Box::leak(
                vec![
                    CString::new("setun_dos").map_err(|e| e.to_string())?,
                    CString::new("-iwad").map_err(|e| e.to_string())?,
                    CString::new(wad_path).map_err(|e| e.to_string())?,
                ]
                .into_boxed_slice(),
            );
            let mut c_args: Vec<*mut std::ffi::c_char> =
                args.iter().map(|s| s.as_ptr() as *mut _).collect();
            c_args.push(std::ptr::null_mut());
            let leaked_argv: &'static mut [*mut std::ffi::c_char] =
                Box::leak(c_args.into_boxed_slice());

            // SAFETY: doomgeneric_Create receives leaked null-terminated argv strings valid for program lifetime.
            unsafe {
                doomgeneric_Create((leaked_argv.len() - 1) as i32, leaked_argv.as_mut_ptr());
            }
            DOOM_INITIALIZED.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Steps one tick/frame of the native Doom engine and updates `last_frame_pixels`.
    pub fn step_native_tick(&mut self) -> Result<&[u8], String> {
        let _lock = DOOM_GLOBAL_LOCK
            .lock()
            .map_err(|e| format!("Failed to acquire Doom lock: {}", e))?;

        if !DOOM_INITIALIZED.load(Ordering::SeqCst) {
            return Err("Native Doom engine is not initialized".to_string());
        }

        // SAFETY: doomgeneric_Tick executes an authentic engine frame tick under DOOM_GLOBAL_LOCK.
        // I_VideoBuffer is verified non-null before constructing a 320x200 byte slice.
        unsafe {
            doomgeneric_Tick();
            if I_VideoBuffer.is_null() {
                return Err("I_VideoBuffer is null after doomgeneric_Tick".to_string());
            }
            let slice = std::slice::from_raw_parts(I_VideoBuffer, 320 * 200);
            self.last_frame_pixels.copy_from_slice(slice);
        }

        self.frame_counter += 1;
        self.dos.tick_timer();

        Ok(&self.last_frame_pixels)
    }

    /// Injects a keyboard event directly into the Doom input queue via Setun FFI.
    pub fn inject_doom_key(&mut self, pressed: bool, doom_key: u8) {
        crate::mmo::setun_PushKeyEvent(if pressed { 1 } else { 0 }, doom_key);
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
            self.dos
                .mem
                .write_word(frame_buffer_addr + (i as i64), color.to_word21())
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
        self.last_frame_pixels
            .copy_from_slice(frame_palette_indices);
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

/// Translates common SDL2 keycodes into Doom scancodes (`doomkeys.h`).
pub fn convert_sdl_to_doom_key(sdl_key: i32) -> Option<u8> {
    match sdl_key {
        // Enter / Return
        13 | 1073741912 => Some(13),
        // Escape
        27 => Some(27),
        // Movement: W / Up
        119 | 1073741906 => Some(0xad), // KEY_UPARROW
        // Movement: S / Down
        115 | 1073741905 => Some(0xaf), // KEY_DOWNARROW
        // Movement: A / Left
        97 | 1073741904 => Some(0xac), // KEY_LEFTARROW
        // Movement: D / Right
        100 | 1073741903 => Some(0xae), // KEY_RIGHTARROW
        // Fire: Ctrl
        1073742048 | 1073742052 => Some(0xa3), // KEY_FIRE
        // Use / Open Door: Space or E
        32 | 101 => Some(0xa2), // KEY_USE
        // Run: Shift
        1073742049 | 1073742053 => Some(0x80 + 0x36), // KEY_RSHIFT
        // Strafe: Alt
        1073742050 | 1073742054 => Some(0x80 + 0x38), // KEY_RALT
        // Tab: Automap
        9 => Some(9), // KEY_TAB
        // Backspace
        8 => Some(0x7f),
        // Weapons 1-7
        k @ 49..=55 => Some(k as u8),
        // Confirmation keys
        121 | 89 => Some(b'y'),
        110 | 78 => Some(b'n'),
        _ => None,
    }
}
