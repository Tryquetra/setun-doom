//! Integration test suite for Setun-DOS (Level 3 Full-System OS).
//!
//! Validates the static memory map, trichotomic system call dispatcher,
//! flat executable loading (`TFX`), cooperative task scheduler, and Doom runner.

use std::fs;
use std::path::Path;

use setun_doom::cpu::{Instruction, Opcode};
use setun_doom::dos::doom_runner::DoomDosRunner;
use setun_doom::dos::memmap::*;
use setun_doom::dos::syscall::*;
use setun_doom::dos::tfx::TfxExecutable;
use setun_doom::dos::SetunDos;
use setun_doom::word::{Tryte6, Word21};

#[test]
fn test_setun_dos_boot_and_memory_map() {
    let mut dos = SetunDos::new();
    assert!(!dos.is_booted);

    dos.boot().expect("Kernel boot failed");
    assert!(dos.is_booted);

    // Verify TIVT table has been initialized
    let tivt_word = dos.mem.read_word(TIVT_BASE).expect("Failed to read TIVT");
    assert_ne!(tivt_word, Word21::ZERO);

    // Verify VGA Framebuffer is initialized to black
    let fb_pixel = dos.mem.read_word(FRAMEBUFFER_BASE).expect("Failed to read FB");
    assert_eq!(fb_pixel, Word21::ZERO);

    // Verify boundary helper functions
    assert!(is_user_address(0));
    assert!(is_user_address(1_999_999));
    assert!(!is_user_address(2_000_000));

    assert!(is_framebuffer_address(4_600_000));
    assert!(is_framebuffer_address(4_600_000 + (320 * 200) - 1));
    assert!(!is_framebuffer_address(4_600_000 + (320 * 200)));
}

#[test]
fn test_setun_dos_syscalls_and_trichotomy() {
    let mut dos = SetunDos::new();
    dos.boot().expect("Boot failed");

    // 1. Test SYS_SET_PIXEL and SYS_GET_PIXEL
    dos.cpu.acc = Word21::new(SYS_SET_PIXEL);
    dos.cpu.index_regs[0] = Word21::new(10); // x
    dos.cpu.index_regs[1] = Word21::new(20); // y
    dos.cpu.index_regs[2] = Word21::new(123); // color index 123
    let res = dos.dispatch_current_syscall(SYS_SET_PIXEL);
    assert_eq!(res, SyscallResult::Continue(Word21::ZERO));

    // Read back via SYS_GET_PIXEL
    dos.cpu.acc = Word21::new(SYS_GET_PIXEL);
    dos.cpu.index_regs[0] = Word21::new(10);
    dos.cpu.index_regs[1] = Word21::new(20);
    dos.dispatch_current_syscall(SYS_GET_PIXEL);
    let expected_color = Tryte6::from_palette_index(123).to_word21();
    assert_eq!(dos.cpu.acc, expected_color);

    // 2. Test boundary violation on SYS_SET_PIXEL: x = 999 must return -1 in ACC
    dos.cpu.acc = Word21::new(SYS_SET_PIXEL);
    dos.cpu.index_regs[0] = Word21::new(999);
    dos.cpu.index_regs[1] = Word21::new(20);
    dos.cpu.index_regs[2] = Word21::new(100);
    let res = dos.dispatch_current_syscall(SYS_SET_PIXEL);
    assert_eq!(res, SyscallResult::Continue(Word21::new(-1)));

    // 3. Test SYS_DRAW_WINDOW: draw a 50x30 window at (5, 5)
    dos.cpu.acc = Word21::new(SYS_DRAW_WINDOW);
    dos.cpu.index_regs[0] = Word21::new(5);
    dos.cpu.index_regs[1] = Word21::new(5);
    dos.cpu.index_regs[2] = Word21::new(50);
    dos.cpu.index_regs[3] = Word21::new(30);
    dos.cpu.index_regs[4] = Word21::new(200); // color 200
    let res = dos.dispatch_current_syscall(SYS_DRAW_WINDOW);
    assert_eq!(res, SyscallResult::Continue(Word21::ZERO));

    // Verify top-left border pixel was set
    let border_addr = FRAMEBUFFER_BASE + 5 * (FRAMEBUFFER_WIDTH as i64) + 5;
    assert_eq!(
        dos.mem.read_word(border_addr).unwrap(),
        Tryte6::from_palette_index(200).to_word21()
    );

    // 4. Test SYS_GET_KEY: empty returns 0
    dos.cpu.acc = Word21::new(SYS_GET_KEY);
    let res = dos.dispatch_current_syscall(SYS_GET_KEY);
    assert_eq!(res, SyscallResult::Continue(Word21::ZERO));

    // Inject key 'W' (87) and read back -> must return +1 in ACC and 87 in IX0
    dos.inject_key(87);
    dos.cpu.acc = Word21::new(SYS_GET_KEY);
    let res = dos.dispatch_current_syscall(SYS_GET_KEY);
    assert_eq!(res, SyscallResult::Continue(Word21::new(1)));
    assert_eq!(dos.cpu.index_regs[0].as_i64(), 87);

    // 5. Test SYS_ALLOC: bump heap allocation
    dos.cpu.acc = Word21::new(SYS_ALLOC);
    dos.cpu.index_regs[0] = Word21::new(1024);
    let res = dos.dispatch_current_syscall(SYS_ALLOC);
    assert!(matches!(res, SyscallResult::Continue(addr) if addr.as_i64() >= 500_000));
}

#[test]
fn test_setun_dos_tfx_executable_lifecycle() {
    let mut dos = SetunDos::new();
    dos.boot().expect("Boot failed");

    // Assemble a program that:
    // 1. Sets ACC = SYS_SET_PIXEL (1)
    // 2. Sets IX0 = 40 (x), IX1 = 50 (y), IX2 = 77 (color)
    // 3. Executes SYS
    // 4. Sets ACC = SYS_EXIT (255)
    // 5. Executes SYS
    let instructions = [
        Instruction::new(Opcode::LoadImm, 0, SYS_SET_PIXEL),
        Instruction::new(Opcode::LoadIxImm, 0, 40),
        Instruction::new(Opcode::LoadIxImm, 1, 50),
        Instruction::new(Opcode::LoadIxImm, 2, 77),
        Instruction::new(Opcode::Syscall, 0, 0),
        Instruction::new(Opcode::LoadImm, 0, SYS_EXIT),
        Instruction::new(Opcode::LoadIxImm, 0, 0),
        Instruction::new(Opcode::Syscall, 0, 0),
        Instruction::new(Opcode::Halt, 0, 0),
    ];

    let tfx = TfxExecutable::from_instructions(100, &instructions, USER_DEFAULT_STACK);

    // Test byte roundtrip serialization
    let bytes = tfx.to_bytes();
    let deserialized = TfxExecutable::from_bytes(&bytes).expect("Failed to deserialize TFX bytes");
    assert_eq!(tfx, deserialized);

    // Load executable into Setun-DOS
    let pid = dos.load_executable(&tfx, "draw_test.tfx").expect("Failed to load TFX");
    assert_eq!(pid, 1);

    // Run until exit
    let cycles = dos.run_ticks(100);
    assert!(cycles > 0);

    // Verify pixel was written to framebuffer
    let target_addr = FRAMEBUFFER_BASE + 50 * (FRAMEBUFFER_WIDTH as i64) + 40;
    let pixel = dos.mem.read_word(target_addr).unwrap();
    assert_eq!(pixel, Tryte6::from_palette_index(77).to_word21());
}

#[test]
fn test_setun_dos_doom_runner_and_wad_mounting() {
    let wad_path = Path::new("vendor/DOOM1.WAD");
    let wad_bytes = if wad_path.exists() {
        fs::read(wad_path).ok()
    } else {
        None
    };

    let mut runner = DoomDosRunner::new(wad_bytes.as_deref()).expect("Failed to create Doom runner");

    if wad_bytes.is_some() {
        assert!(!runner.dos.fs.files.is_empty(), "WAD lumps were not mounted");
        // Verify key lumps exist
        assert!(runner.dos.fs.find("PLAYPAL").is_some(), "PLAYPAL lump missing");
    }

    // Render 10 simulated frames and inject keys
    runner.inject_key(119); // 'w'
    runner.inject_key(32);  // space

    let mut test_frame = vec![12u8; 320 * 200];
    for frame in 0..10 {
        test_frame[0] = frame as u8;
        runner.render_frame(&test_frame).expect("Failed to render Doom frame");
    }

    assert_eq!(runner.frame_counter, 10);
    assert_eq!(runner.dos.timer_ticks, 10);

    // Verify screen RGBA output contains valid data
    let rgba = runner.get_screen_rgba();
    assert_eq!(rgba.len(), 320 * 200 * 4);
}
