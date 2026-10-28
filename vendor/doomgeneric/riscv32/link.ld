/* Linker script for the RISC-V guest executed by the Setun-Doom DBT.
 *
 * Guest virtual address 0 maps to Setun word 100,000 (one word per byte), so the
 * code, heap and stack must stay below the 4,782,969-word memory ceiling. The
 * framebuffer lives at word 4,600,000 and is written by the host, never by the guest.
 */
OUTPUT_ARCH(riscv)
ENTRY(_start)

MEMORY
{
    guest (rwx) : ORIGIN = 0x00000000, LENGTH = 0x00450000
}

SECTIONS
{
    .text : {
        *(.text.start)
        *(.text*)
    } > guest

    .rodata : {
        *(.rodata*)
        *(.srodata*)
    } > guest

    .data : {
        *(.data*)
        *(.sdata*)
    } > guest

    .bss (NOLOAD) : {
        _bss_start = .;
        *(.bss*)
        *(.sbss*)
        *(COMMON)
        _bss_end = .;
    } > guest

    .heap (NOLOAD) : {
        . = ALIGN(16);
        _heap_start = .;
        . = 0x00430000;
        _heap_end = .;
    } > guest

    __global_pointer$ = _heap_start + 0x800;
    _end = .;
}
