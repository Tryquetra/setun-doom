// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! C ABI artifact for the `doomgeneric` engine (`libsetun_ffi.a`, `libsetun_ffi.so`).
//!
//! The emulator lives in the `setun-doom` crate. This crate exists so the `staticlib` and
//! `cdylib` artifact types stay out of that manifest: `setun-doom` must remain `rlib` only,
//! because the bare-metal RISC-V unikernel in `crates/setun-kernel` builds it for
//! `riscv64gc-unknown-none-elf`, where a final artifact would demand a global allocator and a
//! panic handler that belong to the kernel.
//!
//! Every symbol below is re-exported so the linker keeps the `#[no_mangle]` definitions when it
//! archives the dependency.

pub use setun_doom::mmo::{
    DG_DrawFrame, DG_DrawFrameToBuffer, DG_GetKey, DG_GetTicksMs, DG_Init, DG_SetWindowTitle,
    DG_SleepMs, SetunHookCounters, get_active_palette, global_machine, lock_machine,
    setun_AproxDistance,
    setun_BlitVideoBuffer, setun_DrawColumn, setun_DrawFuzzColumn, setun_DrawSpan,
    setun_DrawTranslatedColumn, setun_FixedDiv, setun_FixedMul, setun_GetLiveTelemetrySnapshot,
    setun_GetHookCounters, setun_PointOnSide, setun_PrintTelemetry, setun_PushKeyEvent,
    setun_SyncPalette,
};
