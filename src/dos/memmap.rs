//! Static linear memory layout for Setun-DOS inspired by KolibriOS `memmap.inc`.
//!
//! Maps user space, kernel structures, interrupt vector tables, and VGA
//! framebuffer into the 21-trit $3^{14} = 4,782,969$ word space.

/// Base address of user execution space.
pub const USER_SPACE_BASE: i64 = 0;

/// Limit of user execution space (2,000,000 words).
pub const USER_SPACE_LIMIT: i64 = 2_000_000;

/// Default stack pointer for user tasks (descends toward base).
pub const USER_DEFAULT_STACK: i64 = 1_999_999;

/// Base address of kernel space.
pub const KERNEL_SPACE_BASE: i64 = 2_000_000;

/// Ternary Interrupt Vector Table (TIVT) with 256 vector pointers.
pub const TIVT_BASE: i64 = 2_000_001;

/// Process Control Block (PCB) table base address.
pub const PCB_TABLE_BASE: i64 = 2_001_000;

/// Window manager data structures base address (up to 256 windows).
pub const WINDOW_DATA_BASE: i64 = 2_010_000;

/// Virtual filesystem file descriptor table base address.
pub const FILE_TABLE_BASE: i64 = 2_020_000;

/// WAD data lump cache buffer base address.
pub const WAD_CACHE_BASE: i64 = 2_100_000;

/// Kernel binary and runtime code base address.
pub const KERNEL_CODE_BASE: i64 = 3_000_000;

/// Direct VGA Framebuffer base address (Tryte6 cells).
pub const FRAMEBUFFER_BASE: i64 = 4_600_000;

/// Screen width in pixels.
pub const FRAMEBUFFER_WIDTH: usize = 320;

/// Screen height in pixels.
pub const FRAMEBUFFER_HEIGHT: usize = 200;

/// Total pixel count in screen framebuffer.
pub const FRAMEBUFFER_PIXELS: usize = FRAMEBUFFER_WIDTH * FRAMEBUFFER_HEIGHT;

/// Checks if an address falls within valid user memory.
#[inline(always)]
pub const fn is_user_address(addr: i64) -> bool {
    addr >= USER_SPACE_BASE && addr < USER_SPACE_LIMIT
}

/// Checks if an address falls within the VGA framebuffer memory.
#[inline(always)]
pub const fn is_framebuffer_address(addr: i64) -> bool {
    addr >= FRAMEBUFFER_BASE && addr < FRAMEBUFFER_BASE + (FRAMEBUFFER_PIXELS as i64)
}
