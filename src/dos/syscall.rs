//! System call interface and trichotomic dispatch table for Setun-DOS.
//!
//! Models KolibriOS `servetable2` using Setun 21-trit register conventions:
//! - Syscall number passed in `ACC`.
//! - Arguments passed in index registers `IX0` through `IX5`.
//! - Return code returned in `ACC` using trichotomic states:
//!   - `0`: Success / Idle.
//!   - `-1`: Error / Fault / Boundary violation.
//!   - `+1` or `> 0`: Value present / Bytes read / Key code / Allocated address.

use crate::cpu::Cpu;
use crate::dos::memmap::*;
use crate::memory::Memory;
use crate::word::{Tryte6, Word21};

pub const SYS_DRAW_WINDOW: i64 = 0;
pub const SYS_SET_PIXEL: i64 = 1;
pub const SYS_GET_KEY: i64 = 2;
pub const SYS_YIELD: i64 = 3;
pub const SYS_FILE_READ: i64 = 4;
pub const SYS_ALLOC: i64 = 5;
pub const SYS_BLIT: i64 = 6;
pub const SYS_GET_TICKS: i64 = 7;
pub const SYS_PRINT_STR: i64 = 8;
pub const SYS_GET_PIXEL: i64 = 9;
pub const SYS_EXIT: i64 = 255;

/// Syscall execution result for the Setun-DOS kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyscallResult {
    /// Continue execution of current task with return value in `ACC`.
    Continue(Word21),
    /// Current task yielded its time slice.
    Yield,
    /// Current task requested termination with exit code.
    Exit(i64),
    /// Unimplemented or invalid system call vector.
    InvalidSyscall(i64),
}

/// Dispatches a system call using Setun-DOS register state.
pub struct SyscallDispatcher;

impl SyscallDispatcher {
    /// Executes a system call directly from CPU registers and Memory.
    pub fn dispatch(
        sys_num: i64,
        cpu: &mut Cpu,
        mem: &mut Memory,
        key_buffer: &mut Vec<i64>,
        heap_brk: &mut i64,
        timer_ticks: u64,
        file_storage: &[(String, Vec<Word21>)],
    ) -> SyscallResult {
        match sys_num {
            SYS_DRAW_WINDOW => {
                let x = cpu.index_regs[0].as_i64();
                let y = cpu.index_regs[1].as_i64();
                let w = cpu.index_regs[2].as_i64();
                let h = cpu.index_regs[3].as_i64();
                let color_idx = cpu.index_regs[4].as_i64();

                if x < 0 || y < 0 || w <= 0 || h <= 0 || x + w > (FRAMEBUFFER_WIDTH as i64) || y + h > (FRAMEBUFFER_HEIGHT as i64) {
                    return SyscallResult::Continue(Word21::new(-1));
                }

                let color = Tryte6::from_palette_index(color_idx as u8);
                // Draw top and bottom borders
                for col in x..(x + w) {
                    let top_addr = FRAMEBUFFER_BASE + y * (FRAMEBUFFER_WIDTH as i64) + col;
                    let bot_addr = FRAMEBUFFER_BASE + (y + h - 1) * (FRAMEBUFFER_WIDTH as i64) + col;
                    let _ = mem.write_word(top_addr, color.to_word21());
                    let _ = mem.write_word(bot_addr, color.to_word21());
                }
                // Draw left and right borders
                for row in y..(y + h) {
                    let left_addr = FRAMEBUFFER_BASE + row * (FRAMEBUFFER_WIDTH as i64) + x;
                    let right_addr = FRAMEBUFFER_BASE + row * (FRAMEBUFFER_WIDTH as i64) + (x + w - 1);
                    let _ = mem.write_word(left_addr, color.to_word21());
                    let _ = mem.write_word(right_addr, color.to_word21());
                }

                SyscallResult::Continue(Word21::ZERO)
            }

            SYS_SET_PIXEL => {
                let x = cpu.index_regs[0].as_i64();
                let y = cpu.index_regs[1].as_i64();
                let color_idx = cpu.index_regs[2].as_i64();

                if x < 0 || x >= (FRAMEBUFFER_WIDTH as i64) || y < 0 || y >= (FRAMEBUFFER_HEIGHT as i64) {
                    return SyscallResult::Continue(Word21::new(-1));
                }

                let addr = FRAMEBUFFER_BASE + y * (FRAMEBUFFER_WIDTH as i64) + x;
                let color = Tryte6::from_palette_index(color_idx as u8);
                let _ = mem.write_word(addr, color.to_word21());
                SyscallResult::Continue(Word21::ZERO)
            }

            SYS_GET_PIXEL => {
                let x = cpu.index_regs[0].as_i64();
                let y = cpu.index_regs[1].as_i64();

                if x < 0 || x >= (FRAMEBUFFER_WIDTH as i64) || y < 0 || y >= (FRAMEBUFFER_HEIGHT as i64) {
                    return SyscallResult::Continue(Word21::new(-1));
                }

                let addr = FRAMEBUFFER_BASE + y * (FRAMEBUFFER_WIDTH as i64) + x;
                match mem.read_word(addr) {
                    Ok(val) => SyscallResult::Continue(val),
                    Err(_) => SyscallResult::Continue(Word21::new(-1)),
                }
            }

            SYS_GET_KEY => {
                if key_buffer.is_empty() {
                    // 0 = No key waiting
                    SyscallResult::Continue(Word21::ZERO)
                } else {
                    let key = key_buffer.remove(0);
                    cpu.index_regs[0] = Word21::new(key);
                    // +1 = Key successfully retrieved
                    SyscallResult::Continue(Word21::new(1))
                }
            }

            SYS_YIELD => {
                SyscallResult::Yield
            }

            SYS_FILE_READ => {
                let handle = cpu.index_regs[0].as_i64() as usize;
                let offset = cpu.index_regs[1].as_i64() as usize;
                let dst_addr = cpu.index_regs[2].as_i64();
                let count = cpu.index_regs[3].as_i64() as usize;

                if handle >= file_storage.len() {
                    return SyscallResult::Continue(Word21::new(-1));
                }

                let file_data = &file_storage[handle].1;
                if offset >= file_data.len() {
                    return SyscallResult::Continue(Word21::ZERO);
                }

                let available = (file_data.len() - offset).min(count);
                for i in 0..available {
                    let word = file_data[offset + i];
                    if mem.write_word(dst_addr + (i as i64), word).is_err() {
                        return SyscallResult::Continue(Word21::new(-1));
                    }
                }

                // Return words read
                SyscallResult::Continue(Word21::new(available as i64))
            }

            SYS_ALLOC => {
                let words_needed = cpu.index_regs[0].as_i64();
                if words_needed <= 0 || *heap_brk + words_needed >= USER_SPACE_LIMIT {
                    return SyscallResult::Continue(Word21::new(-1));
                }

                let allocated_base = *heap_brk;
                *heap_brk += words_needed;
                SyscallResult::Continue(Word21::new(allocated_base))
            }

            SYS_BLIT => {
                let src_addr = cpu.index_regs[0].as_i64();
                let dst_x = cpu.index_regs[1].as_i64();
                let dst_y = cpu.index_regs[2].as_i64();
                let width = cpu.index_regs[3].as_i64();
                let height = cpu.index_regs[4].as_i64();

                if dst_x < 0 || dst_y < 0 || width <= 0 || height <= 0 ||
                   dst_x + width > (FRAMEBUFFER_WIDTH as i64) || dst_y + height > (FRAMEBUFFER_HEIGHT as i64) {
                    return SyscallResult::Continue(Word21::new(-1));
                }

                for row in 0..height {
                    for col in 0..width {
                        let src_idx = src_addr + row * width + col;
                        let dst_idx = FRAMEBUFFER_BASE + (dst_y + row) * (FRAMEBUFFER_WIDTH as i64) + (dst_x + col);
                        if let Ok(pixel) = mem.read_word(src_idx) {
                            let _ = mem.write_word(dst_idx, pixel);
                        }
                    }
                }

                SyscallResult::Continue(Word21::ZERO)
            }

            SYS_GET_TICKS => {
                SyscallResult::Continue(Word21::new(timer_ticks as i64))
            }

            SYS_PRINT_STR => {
                let str_ptr = cpu.index_regs[0].as_i64();
                let mut x = cpu.index_regs[1].as_i64();
                let y = cpu.index_regs[2].as_i64();
                let color_idx = cpu.index_regs[3].as_i64();
                let color = Tryte6::from_palette_index(color_idx as u8);

                let mut count = 0;
                let mut curr = str_ptr;
                while let Ok(val) = mem.read_word(curr) {
                    let ch = val.as_i64();
                    if ch == 0 || x + 4 >= (FRAMEBUFFER_WIDTH as i64) || y + 6 >= (FRAMEBUFFER_HEIGHT as i64) {
                        break;
                    }
                    // Simple 3x5 block character representation
                    for dy in 0..5 {
                        for dx in 0..3 {
                            let addr = FRAMEBUFFER_BASE + (y + dy) * (FRAMEBUFFER_WIDTH as i64) + (x + dx);
                            let _ = mem.write_word(addr, color.to_word21());
                        }
                    }
                    x += 4;
                    curr += 1;
                    count += 1;
                }

                SyscallResult::Continue(Word21::new(count))
            }

            SYS_EXIT => {
                let code = cpu.index_regs[0].as_i64();
                SyscallResult::Exit(code)
            }

            unknown => SyscallResult::InvalidSyscall(unknown),
        }
    }
}
