//! Setun-DOS: Authentic 21-Trit Operating Environment inspired by KolibriOS.
//!
//! Features static memory mapping, trichotomic system call dispatch,
//! flat executable loading (`TFX`), cooperative task switching,
//! and native hardware framebuffer rendering.

pub mod desktop;
#[cfg(feature = "std")]
pub mod doom_runner;
pub mod fs;
pub mod memmap;
pub mod syscall;
pub mod task;
pub mod tfx;

pub use desktop::{ActiveWindow, SetunDesktop};

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, string::ToString, vec::Vec};

use crate::cpu::{Cpu, CpuTrap};
use crate::dos::fs::SetunFileSystem;
use crate::dos::memmap::{FRAMEBUFFER_BASE, FRAMEBUFFER_PIXELS, KERNEL_CODE_BASE, TIVT_BASE};
use crate::dos::syscall::{KernelGuiState, SyscallDispatcher, SyscallResult};
use crate::dos::task::{TaskScheduler, TaskState};
use crate::dos::tfx::TfxExecutable;
use crate::memory::Memory;
use crate::word::Word21;

/// The Setun-DOS kernel instance.
pub struct SetunDos {
    pub cpu: Cpu,
    pub mem: Memory,
    pub scheduler: TaskScheduler,
    pub fs: SetunFileSystem,
    pub key_buffer: Vec<i64>,
    pub gui_state: KernelGuiState,
    pub heap_brk: i64,
    pub timer_ticks: u64,
    pub is_booted: bool,
}

impl SetunDos {
    /// Creates an uninitialized Setun-DOS instance.
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            mem: Memory::new(),
            scheduler: TaskScheduler::new(),
            fs: SetunFileSystem::new(),
            key_buffer: Vec::new(),
            gui_state: KernelGuiState::default(),
            heap_brk: 500_000,
            timer_ticks: 0,
            is_booted: false,
        }
    }

    /// Boots the kernel, initializing the Ternary Interrupt Vector Table (TIVT)
    /// and video memory according to `memmap.setun`.
    pub fn boot(&mut self) -> Result<(), String> {
        // 1. Initialize TIVT with default system call handler vector at entry 0x40
        for i in 0..256 {
            let addr = TIVT_BASE + i;
            let handler_pc = KERNEL_CODE_BASE + i * 4;
            self.mem.write_word(addr, Word21::new(handler_pc)).map_err(|e| {
                format!("Failed to write TIVT vector {}: {:?}", i, e)
            })?;
        }

        // 2. Clear VGA Framebuffer to palette index 0 (black)
        let zero_pixel = Word21::ZERO;
        for i in 0..FRAMEBUFFER_PIXELS {
            let addr = FRAMEBUFFER_BASE + (i as i64);
            self.mem.write_word_fast(addr, zero_pixel);
        }

        self.key_buffer.clear();
        self.timer_ticks = 0;
        self.is_booted = true;
        Ok(())
    }

    /// Mounts an authentic Doom IWAD/PWAD byte buffer and initializes the 256-color VGA palette from `PLAYPAL`.
    pub fn mount_wad(&mut self, wad_bytes: &[u8]) -> Result<usize, String> {
        let count = self.fs.mount_wad(wad_bytes)?;
        if let Some(idx) = self.fs.find("PLAYPAL") {
            let bytes: Vec<u8> = self.fs.files[idx]
                .words
                .iter()
                .take(768)
                .map(|w| w.as_i64() as u8)
                .collect();
            if bytes.len() >= 768 {
                self.mem.load_playpal(&bytes);
            }
        }
        Ok(count)
    }

    /// Injects a key code into the hardware input queue.
    pub fn inject_key(&mut self, key_code: i64) {
        self.key_buffer.push(key_code);
    }

    /// Advances system timer by one tick (35 Hz).
    pub fn tick_timer(&mut self) {
        self.timer_ticks += 1;
    }

    /// Loads and spawns a flat executable (`TFX`) into Setun-DOS user memory.
    pub fn load_executable(&mut self, tfx: &TfxExecutable, task_name: &str) -> Result<usize, String> {
        if !self.is_booted {
            return Err("Cannot load executable: Setun-DOS is not booted".to_string());
        }

        tfx.load_into_memory(&mut self.mem, tfx.entry_pc)?;

        let pid = self.scheduler.spawn(
            task_name,
            tfx.entry_pc,
            tfx.stack_top,
            self.heap_brk,
        );

        if self.scheduler.tasks.len() == 1 {
            // First task becomes active immediately
            if let Some(task) = self.scheduler.current_mut() {
                task.state = TaskState::Running;
                task.restore_context(&mut self.cpu);
            }
        }

        Ok(pid)
    }

    /// Manually dispatches a system call through the kernel dispatcher.
    pub fn dispatch_current_syscall(&mut self, sys_num: i64) -> SyscallResult {
        let storage = self.fs.as_storage_slice();
        let res = SyscallDispatcher::dispatch(
            sys_num,
            &mut self.cpu,
            &mut self.mem,
            &mut self.key_buffer,
            &mut self.heap_brk,
            self.timer_ticks,
            &storage,
            &mut self.gui_state,
        );

        match res {
            SyscallResult::Continue(ret_val) => {
                self.cpu.resume_from_syscall(ret_val);
                SyscallResult::Continue(ret_val)
            }
            SyscallResult::Yield => {
                self.cpu.resume_from_syscall(Word21::ZERO);
                self.scheduler.switch_next(&mut self.cpu, &mut self.heap_brk);
                SyscallResult::Yield
            }
            SyscallResult::Exit(code) => {
                if let Some(task) = self.scheduler.current_mut() {
                    task.state = TaskState::Exited(code);
                }
                self.scheduler.switch_next(&mut self.cpu, &mut self.heap_brk);
                SyscallResult::Exit(code)
            }
            SyscallResult::InvalidSyscall(num) => {
                self.cpu.resume_from_syscall(Word21::new(-1));
                SyscallResult::InvalidSyscall(num)
            }
        }
    }

    /// Executes a single CPU step with automatic system call interception.
    /// Returns `Ok(true)` if execution continues, `Ok(false)` if halted or all tasks exited,
    /// or `Err(CpuTrap)` on hardware fault.
    pub fn step(&mut self) -> Result<bool, CpuTrap> {
        match self.cpu.step_checked(&mut self.mem) {
            Ok(running) => Ok(running),
            Err(CpuTrap::Syscall { pc: _, num }) => {
                self.dispatch_current_syscall(num);
                // Continue execution unless CPU is explicitly halted
                Ok(!self.cpu.halted)
            }
            Err(trap) => Err(trap),
        }
    }

    /// Runs up to `max_steps` CPU instructions with automated system call handling.
    pub fn run_ticks(&mut self, max_steps: usize) -> usize {
        let mut steps = 0;
        while steps < max_steps {
            match self.step() {
                Ok(true) => steps += 1,
                Ok(false) => break,
                Err(_) => break,
            }
        }
        steps
    }
}

impl Default for SetunDos {
    fn default() -> Self {
        Self::new()
    }
}
