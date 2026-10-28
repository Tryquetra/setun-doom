#![no_std]
#![no_main]
#![warn(let_underscore_drop, clippy::let_underscore_must_use)]

extern crate alloc;

mod allocator;
mod sbi;
#[macro_use]
mod uart;
mod vga;

use core::arch::global_asm;
use core::panic::PanicInfo;

use setun_doom::dos::memmap::{FRAMEBUFFER_BASE, TIVT_BASE, USER_SPACE_LIMIT};
use setun_doom::dos::tfx::TfxExecutable;
use setun_doom::dos::{SetunDesktop, SetunDos};
use setun_doom::{ActiveWindow, Instruction, Opcode, Word21};

global_asm!(include_str!("boot.S"));

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("\n============================================================");
    kprintln!("  KERNEL PANIC ON RISC-V SUPERVISOR");
    kprintln!("============================================================");
    if let Some(loc) = info.location() {
        kprintln!("  Location: {}:{}", loc.file(), loc.line());
    }
    kprintln!("  Payload:  {}", info.message());
    kprintln!("============================================================");
    sbi::sbi_shutdown();
}

#[no_mangle]
pub extern "C" fn kmain(hart_id: usize, dtb_addr: usize) -> ! {
    uart::Uart::init();

    kprintln!("============================================================");
    kprintln!("  SETUN-DOS BARE-METAL RISC-V UNIKERNEL (LEVEL 3)           ");
    kprintln!("  Soviet Balanced Ternary Operating Environment on QEMU    ");
    kprintln!("============================================================");
    kprintln!(" * Target Architecture : RISC-V 64-bit (riscv64gc-unknown-none-elf)");
    kprintln!(" * Privilege Mode      : Supervisor Mode (S-Mode via OpenSBI)");
    kprintln!(" * Primary Hart ID     : {}", hart_id);
    kprintln!(" * Device Tree Blob    : 0x{:016x}", dtb_addr);

    // 1. Initialize Heap
    let (heap_start, heap_end) = allocator::init_heap();
    kprintln!(" * Dynamic Heap Pool   : 0x{:08x} - 0x{:08x} (128 MB)", heap_start, heap_end);

    // 2. Boot Setun-DOS Kernel
    kprintln!("-> Initializing Setun-DOS 21-trit kernel...");
    let mut dos = SetunDos::new();
    if let Err(e) = dos.boot() {
        kprintln!("Kernel Panic: Failed to boot Setun-DOS: {}", e);
        sbi::sbi_shutdown();
    }

    kprintln!("   [OK] TIVT Table initialized at address {}", TIVT_BASE);
    kprintln!("   [OK] VGA Framebuffer cleared at address {}", FRAMEBUFFER_BASE);
    kprintln!("   [OK] Linear user space bounded at 0..{}", USER_SPACE_LIMIT);

    // 3. Initialize Retro Desktop Environment
    kprintln!("-> Initializing Setun-DOS Desktop Environment & Icon Manager...");
    let mut desktop = SetunDesktop::new();
    desktop.init_desktop_buttons(&mut dos);
    kprintln!("   [OK] Desktop icon handles registered: DOOM, SHELL, TELEMETRY, ABOUT");

    // 4. Create and load a flat ternary executable (TFX)
    kprintln!("-> Assembling and spawning demonstration TFX executable...");
    let test_instructions = alloc::vec![
        Instruction { opcode: Opcode::Load, reg_idx: 0, operand: 100 },
        Instruction { opcode: Opcode::Add, reg_idx: 0, operand: 101 },
        Instruction { opcode: Opcode::Store, reg_idx: 0, operand: 102 },
        Instruction { opcode: Opcode::Nop, reg_idx: 0, operand: 0 },
    ];
    let tfx = TfxExecutable::from_instructions(1000, &test_instructions, 50_000);

    // Seed test memory cells
    dos.mem
        .write_word(100, Word21::new(42))
        .expect("boot self-test must seed MEM[100]");
    dos.mem
        .write_word(101, Word21::new(13))
        .expect("boot self-test must seed MEM[101]");

    match dos.load_executable(&tfx, "DEMO.TFX") {
        Ok(pid) => {
            kprintln!("   [OK] Task PID {} loaded at PC = {}", pid, tfx.entry_pc);
            kprintln!("-> Executing ternary instructions in Supervisor Mode...");
            let steps = dos.run_ticks(10);
            kprintln!("   [OK] Executed {} ternary cycles.", steps);

            let result_word = dos.mem.read_word(102).unwrap_or(Word21::ZERO);
            kprintln!("   [OK] Arithmetic verification: 42 + 13 = {} (Word21)", result_word.as_i64());

            let tel = dos.cpu.telemetry;
            kprintln!("-> Switching Telemetry Report:");
            kprintln!("   * Trit Flips          : {}", tel.ternary_trit_flips);
            kprintln!("   * Half Swings (0<->1) : {}", tel.ternary_half_swings);
            kprintln!("   * Full Swings (-1<->1): {}", tel.ternary_full_swings);
            kprintln!("   * Carry Propagations  : {}", tel.adder_carry_propagations);
        }
        Err(e) => {
            kprintln!("   [FAIL] Could not load TFX: {}", e);
        }
    }

    // 5. Initialize Hardware Framebuffer
    kprintln!("-> Probing PCI Bus for Bochs/QEMU VGA Display Adapter...");
    let vga = vga::BochsVga::probe_and_init(640, 400);
    if vga.is_some() {
        kprintln!("   [OK] Hardware Framebuffer online: 640x400 (scaled 2x from 320x200)");
    } else {
        kprintln!("   [WARN] No PCI VGA Adapter detected on bus. Running in headless mode.");
    }

    kprintln!("============================================================");
    kprintln!("  SETUN-DOS RISC-V UNIKERNEL RUNNING                        ");
    kprintln!("  Press 'q' to exit, '1'=DOOM, '2'=SHELL, '3'=TELEMETRY, '4'=ABOUT");
    kprintln!("============================================================");

    let mut tick_counter: u64 = 0;
    loop {
        // Handle UART serial input
        if let Some(c) = uart::Uart::getc() {
            match c {
                b'q' | 27 => {
                    kprintln!("-> Exit signal received, powering down virtual machine...");
                    break;
                }
                b'1' => {
                    desktop.active_window = ActiveWindow::Doom;
                    kprintln!(" * Switch Active Window: DOOM");
                }
                b'2' => {
                    desktop.active_window = ActiveWindow::Shell;
                    kprintln!(" * Switch Active Window: SHELL");
                }
                b'3' => {
                    desktop.active_window = ActiveWindow::Telemetry;
                    kprintln!(" * Switch Active Window: TELEMETRY");
                }
                b'4' => {
                    desktop.active_window = ActiveWindow::About;
                    kprintln!(" * Switch Active Window: ABOUT");
                }
                _ => {
                    desktop.handle_key(c as i32, true);
                }
            }
        }

        // Render retro desktop frame into Setun memory
        desktop.render(&mut dos);

        // Blit 320x200 frame scaled 2x to 640x400 physical VGA linear memory
        if let Some(ref v) = vga {
            v.blit_setun_framebuffer_2x(&dos.mem);
        }

        tick_counter += 1;
        if tick_counter.is_multiple_of(350) {
            kprintln!(
                " * Setun-DOS Heartbeat: frame {}, timer ticks {}, active window: {:?}",
                desktop.frame_counter,
                dos.timer_ticks,
                desktop.active_window
            );
        }

        // Delay ~28ms for ~35 FPS
        for _ in 0..60_000 {
            unsafe { core::arch::asm!("nop") };
        }
    }

    // Power down machine via SBI system reset
    sbi::sbi_shutdown();
}
