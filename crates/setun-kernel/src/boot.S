.section .text.entry
.globl _start
_start:
    # a0 = hartid, a1 = dtb
    # Only hart 0 executes, other harts park
    bnez a0, park_hart

    # Setup stack pointer
    la sp, _boot_stack_top

    # Clear BSS
    la t0, _sbss
    la t1, _ebss
clear_bss_loop:
    bgeu t0, t1, clear_bss_done
    sd zero, 0(t0)
    addi t0, t0, 8
    j clear_bss_loop

clear_bss_done:
    # Jump to Rust kernel main
    call kmain

park_hart:
    wfi
    j park_hart

.section .bss.stack
.align 12
.globl _boot_stack
_boot_stack:
    .space 4096 * 16
.globl _boot_stack_top
_boot_stack_top:
