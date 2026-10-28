# Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
# SPDX-License-Identifier: MIT
#
# Boot entry for the RISC-V guest running on the Setun-Doom DBT.
# The engine already places the stack pointer; this only initializes `gp`,
# calls `main` and terminates through the exit syscall.

    .section .text.start
    .globl _start
    .type _start, @function
_start:
    .option push
    .option norelax
    la gp, __global_pointer$
    .option pop
    call main
    li a7, 0
    li a0, 0
    ecall
1:
    j 1b
    .size _start, . - _start
