// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Host service ABI used by the RISC-V guest running on the Setun-Doom DBT.
//!
//! The guest issues `ecall` with the service number in `a7` and arguments in `a0..a2`.
//! `src/dbt.rs` dispatches these numbers to the host platform, which owns the WAD file,
//! the framebuffer, the keyboard queue and the clock.

#ifndef SETUN_RV32_SYSCALL_H
#define SETUN_RV32_SYSCALL_H

#include <stdint.h>

#define SYS_EXIT 0
#define SYS_WRITE 1
#define SYS_TICKS_MS 2
#define SYS_BLIT 3
#define SYS_GETKEY 4
#define SYS_WAD_OPEN 5
#define SYS_WAD_SEEK 6
#define SYS_WAD_READ 7
#define SYS_WAD_SIZE 8
#define SYS_SLEEP_MS 9
#define SYS_PALETTE 10

static inline int32_t setun_syscall(int32_t num, int32_t a0, int32_t a1, int32_t a2)
{
    register int32_t r_a0 asm("a0") = a0;
    register int32_t r_a1 asm("a1") = a1;
    register int32_t r_a2 asm("a2") = a2;
    register int32_t r_a7 asm("a7") = num;

    asm volatile("ecall" : "+r"(r_a0) : "r"(r_a1), "r"(r_a2), "r"(r_a7) : "memory");
    return r_a0;
}

static inline void setun_exit(int32_t status)
{
    setun_syscall(SYS_EXIT, status, 0, 0);
    for (;;) {
    }
}

static inline int32_t setun_write(const void *buffer, int32_t length)
{
    return setun_syscall(SYS_WRITE, (int32_t)(uintptr_t)buffer, length, 0);
}

static inline uint32_t setun_ticks_ms(void)
{
    return (uint32_t)setun_syscall(SYS_TICKS_MS, 0, 0, 0);
}

static inline void setun_blit(const uint8_t *indices)
{
    setun_syscall(SYS_BLIT, (int32_t)(uintptr_t)indices, 0, 0);
}

static inline int32_t setun_getkey(int32_t *pressed, uint8_t *key)
{
    return setun_syscall(SYS_GETKEY, (int32_t)(uintptr_t)pressed, (int32_t)(uintptr_t)key, 0);
}

static inline int32_t setun_wad_open(void)
{
    return setun_syscall(SYS_WAD_OPEN, 0, 0, 0);
}

static inline int32_t setun_wad_seek(int32_t offset)
{
    return setun_syscall(SYS_WAD_SEEK, offset, 0, 0);
}

static inline int32_t setun_wad_read(void *buffer, int32_t length)
{
    return setun_syscall(SYS_WAD_READ, (int32_t)(uintptr_t)buffer, length, 0);
}

static inline int32_t setun_wad_size(void)
{
    return setun_syscall(SYS_WAD_SIZE, 0, 0, 0);
}

static inline void setun_sleep_ms(int32_t ms)
{
    setun_syscall(SYS_SLEEP_MS, ms, 0, 0);
}

static inline void setun_palette(const uint8_t *rgb768)
{
    setun_syscall(SYS_PALETTE, (int32_t)(uintptr_t)rgb768, 0, 0);
}

#endif
