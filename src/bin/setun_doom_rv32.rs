// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Level 1 playable Doom: runs the cross-compiled RV32IM guest on the Setun DBT.
//!
//! The guest (`vendor/doomgeneric/riscv32/doom_rv32.elf`) executes every instruction through the
//! ternary engine and reaches the host through `ecall` services implemented here: framebuffer
//! blit, palette, keyboard, clock and WAD reads. Build the guest with
//! `make -C vendor/doomgeneric/riscv32`.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Instant;

use setun_doom::dbt::{GuestMemory, Rv32Platform, SetunDbtEngine};
use setun_doom::memory::{FB_HEIGHT, FB_SIZE, FB_WIDTH, FRAMEBUFFER_BASE};
use setun_doom::mmo::lock_machine;
use setun_doom::word::Tryte6;
use setun_doom::BenchGui;

const SYS_EXIT: i32 = 0;
const SYS_WRITE: i32 = 1;
const SYS_TICKS_MS: i32 = 2;
const SYS_BLIT: i32 = 3;
const SYS_GETKEY: i32 = 4;
const SYS_WAD_OPEN: i32 = 5;
const SYS_WAD_SEEK: i32 = 6;
const SYS_WAD_READ: i32 = 7;
const SYS_WAD_SIZE: i32 = 8;
const SYS_SLEEP_MS: i32 = 9;
const SYS_PALETTE: i32 = 10;

/// Guest stack top: above the 3.7 MB heap, below the framebuffer at guest `0x0044_AA20`.
const GUEST_STACK_TOP: u32 = 0x0044_a000;

const KEY_RIGHTARROW: u8 = 0xae;
const KEY_LEFTARROW: u8 = 0xac;
const KEY_UPARROW: u8 = 0xad;
const KEY_DOWNARROW: u8 = 0xaf;
const KEY_USE: u8 = 0xa2;
const KEY_FIRE: u8 = 0xa3;
const KEY_ESCAPE: u8 = 27;
const KEY_ENTER: u8 = 13;
const KEY_TAB: u8 = 9;

/// Maps an SDL2 keycode to the Doom key expected by the guest.
fn map_key(sym: i32) -> Option<u8> {
    const SDLK_LEFT: i32 = 1_073_741_904;
    const SDLK_RIGHT: i32 = 1_073_741_903;
    const SDLK_UP: i32 = 1_073_741_906;
    const SDLK_DOWN: i32 = 1_073_741_905;
    const SDLK_LCTRL: i32 = 1_073_742_048;
    const SDLK_RCTRL: i32 = 1_073_742_052;
    const SDLK_LSHIFT: i32 = 1_073_742_049;
    const SDLK_RSHIFT: i32 = 1_073_742_053;

    match sym {
        13 | 1_073_742_021 => Some(KEY_ENTER),
        27 => Some(KEY_ESCAPE),
        9 => Some(KEY_TAB),
        32 => Some(KEY_USE),
        SDLK_LCTRL | SDLK_RCTRL => Some(KEY_FIRE),
        SDLK_LEFT => Some(KEY_LEFTARROW),
        SDLK_RIGHT => Some(KEY_RIGHTARROW),
        SDLK_UP => Some(KEY_UPARROW),
        SDLK_DOWN => Some(KEY_DOWNARROW),
        SDLK_LSHIFT | SDLK_RSHIFT => Some(0x80 + 0x36),
        61 => Some(b'='),
        45 => Some(b'-'),
        97 => Some(KEY_LEFTARROW),
        100 => Some(KEY_RIGHTARROW),
        119 => Some(KEY_UPARROW),
        115 => Some(KEY_DOWNARROW),
        101 => Some(KEY_USE),
        _ if (32..127).contains(&sym) => Some(sym as u8),
        _ => None,
    }
}

/// Host-side services for the RISC-V Doom guest.
struct HostPlatform {
    wad: Option<File>,
    wad_offset: u64,
    wad_len: u64,
    gui: Option<BenchGui>,
    rgba: Vec<u8>,
    start: Instant,
    keys: VecDeque<(bool, u8)>,
    frames: u64,
    tick_calls: u64,
    max_frames: Option<u64>,
}

impl HostPlatform {
    fn new(gui: Option<BenchGui>) -> Self {
        Self {
            wad: None,
            wad_offset: 0,
            wad_len: 0,
            gui,
            rgba: vec![0u8; FB_SIZE * 4],
            start: Instant::now(),
            keys: VecDeque::new(),
            frames: 0,
            tick_calls: 0,
            max_frames: std::env::var("SETUN_RV32_MAX_FRAMES")
                .ok()
                .and_then(|value| value.parse().ok()),
        }
    }

    fn wad_path() -> PathBuf {
        PathBuf::from("vendor/DOOM1.WAD")
    }

    fn poll_keys(&mut self) -> bool {
        let mut events = Vec::new();
        let mut keep_running = true;
        if let Some(gui) = self.gui.as_mut() {
            keep_running =
                gui.poll_events_desktop(|sym, pressed| events.push((sym, pressed)), |_, _| {});
        }
        for (sym, pressed) in events {
            if let Some(key) = map_key(sym) {
                self.keys.push_back((pressed, key));
            }
        }
        keep_running
    }

    fn blit(&mut self, guest: GuestMemory<'_>, pointer: i32) -> i32 {
        let mut frame = vec![0u8; FB_SIZE];
        if guest.read_bytes(pointer as u32, &mut frame).is_err() {
            return -1;
        }

        let mut machine = lock_machine();
        for (i, &index) in frame.iter().enumerate() {
            machine.memory.raw_cells_mut()[FRAMEBUFFER_BASE + i] =
                Tryte6::from_palette_index(index).as_i64();
        }
        machine.memory.render_framebuffer_rgba(&mut self.rgba);
        drop(machine);

        if let Some(gui) = self.gui.as_mut() {
            gui.present_frame(&self.rgba);
        }
        self.frames += 1;
        if self.frames == 1 {
            eprintln!(
                "  [level 1] first frame after {:.1}s",
                self.start.elapsed().as_secs_f64()
            );
        }
        if std::env::var("SETUN_RV32_HEADLESS").is_ok() && self.frames.is_multiple_of(35) {
            eprintln!(
                "  [level 1] frames={} elapsed={:.1}s",
                self.frames,
                self.start.elapsed().as_secs_f64()
            );
        }
        0
    }

    fn palette(&mut self, guest: GuestMemory<'_>, pointer: i32) -> i32 {
        let mut rgb = [0u8; 768];
        if guest.read_bytes(pointer as u32, &mut rgb).is_err() {
            return -1;
        }
        lock_machine().memory.load_playpal(&rgb);
        0
    }

    fn wad_read(&mut self, mut guest: GuestMemory<'_>, buffer: i32, length: i32) -> i32 {
        let Some(file) = self.wad.as_mut() else {
            return -1;
        };
        let requested = length.max(0) as usize;
        if requested == 0 {
            return 0;
        }
        if file.seek(SeekFrom::Start(self.wad_offset)).is_err() {
            return -1;
        }
        let mut data = vec![0u8; requested];
        let read = file.read(&mut data).unwrap_or(0);
        if std::env::var("SETUN_RV32_TRACE_WAD").is_ok() && self.frames < 1 {
            eprintln!(
                "  wad_read offset={} requested={} read={} first={:02x?}",
                self.wad_offset,
                requested,
                read,
                &data[..read.min(16)]
            );
        }
        if guest.write_bytes(buffer as u32, &data[..read]).is_err() {
            return -1;
        }
        self.wad_offset += read as u64;
        read as i32
    }
}

impl Rv32Platform for HostPlatform {
    fn syscall(
        &mut self,
        mut guest: GuestMemory<'_>,
        number: i32,
        a0: i32,
        a1: i32,
        _a2: i32,
    ) -> Option<i32> {
        match number {
            SYS_EXIT => None,
            SYS_WRITE => {
                let mut buffer = vec![0u8; a1.max(0) as usize];
                if guest.read_bytes(a0 as u32, &mut buffer).is_err() {
                    return Some(-1);
                }
                let _ = std::io::stdout().write_all(&buffer);
                Some(a1)
            }
            SYS_TICKS_MS => {
                self.tick_calls += 1;
                if self.tick_calls.is_multiple_of(200) {
                    eprintln!(
                        "  [level 1] ticks={} frames={} elapsed={:.1}s",
                        self.tick_calls,
                        self.frames,
                        self.start.elapsed().as_secs_f64()
                    );
                }
                Some(self.start.elapsed().as_millis() as i32)
            }
            SYS_BLIT => {
                let result = self.blit(guest, a0);
                if self.max_frames.is_some_and(|limit| self.frames >= limit) {
                    return None;
                }
                Some(result)
            }
            SYS_GETKEY => {
                if !self.poll_keys() {
                    return None;
                }
                match self.keys.pop_front() {
                    Some((pressed, key)) => {
                        let pressed_value = i32::from(pressed).to_le_bytes();
                        let key_value = [key];
                        if guest.write_bytes(a0 as u32, &pressed_value).is_err()
                            || guest.write_bytes(a1 as u32, &key_value).is_err()
                        {
                            return Some(0);
                        }
                        Some(1)
                    }
                    None => Some(0),
                }
            }
            SYS_WAD_OPEN => match File::open(Self::wad_path()) {
                Ok(file) => {
                    self.wad_len = file.metadata().map(|m| m.len()).unwrap_or(0);
                    self.wad = Some(file);
                    self.wad_offset = 0;
                    Some(0)
                }
                Err(error) => {
                    eprintln!("setun-doom rv32: cannot open WAD: {error}");
                    Some(-1)
                }
            },
            SYS_WAD_SEEK => {
                self.wad_offset = a0.max(0) as u64;
                Some(self.wad_offset as i32)
            }
            SYS_WAD_READ => Some(self.wad_read(guest, a0, a1)),
            SYS_WAD_SIZE => Some(self.wad_len as i32),
            SYS_SLEEP_MS => {
                // Doom paces itself with 1 ms sleeps; skipping them turns the driver into a
                // throughput benchmark instead of a playable session.
                if a0 > 0 && std::env::var("SETUN_RV32_NO_SLEEP").is_err() {
                    std::thread::sleep(std::time::Duration::from_millis(a0 as u64));
                }
                Some(0)
            }
            SYS_PALETTE => Some(self.palette(guest, a0)),
            _ => Some(-1),
        }
    }
}

fn main() {
    let elf_path = PathBuf::from("vendor/doomgeneric/riscv32/doom_rv32.elf");
    let elf = match std::fs::read(&elf_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "setun-doom rv32: cannot read {}: {error}\nBuild it with: make -C vendor/doomgeneric/riscv32",
                elf_path.display()
            );
            std::process::exit(1);
        }
    };

    let veracity = std::env::var("SETUN_RV32_VERACITY").is_ok();
    let headless = std::env::var("SETUN_RV32_HEADLESS").is_ok();
    let gui = if headless {
        None
    } else {
        BenchGui::new("Setun-Doom Level 1: RV32IM Doom on the 21-Trit DBT")
    };

    if veracity {
        run_guest::<true>(&elf, gui);
    } else {
        run_guest::<false>(&elf, gui);
    }
}

/// Loads the guest in the requested execution mode and runs it until it exits.
fn run_guest<const VERACITY: bool>(elf: &[u8], gui: Option<BenchGui>) {
    let mut engine = match SetunDbtEngine::<VERACITY>::load_elf_with_stack(elf, GUEST_STACK_TOP) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("setun-doom rv32: cannot load guest ELF: {error}");
            std::process::exit(1);
        }
    };
    engine.set_platform(Box::new(HostPlatform::new(gui)));

    println!(
        "Setun-Doom Level 1: RV32IM Doom on the DBT ({}x{}, stack top 0x{:06x}, mode {})",
        FB_WIDTH,
        FB_HEIGHT,
        GUEST_STACK_TOP,
        if VERACITY {
            "veracity (gates + telemetry)"
        } else {
            "fast (direct arithmetic, no telemetry)"
        }
    );

    let started = Instant::now();
    match engine.run(u64::MAX) {
        Ok(report) => {
            println!(
                "guest stopped: {:?} | rv32i instructions = {} | setun instructions = {} | \
                 wall = {:.2}s | fusions = {}+{}+{}",
                report.exit,
                report.rv32i_instructions_executed,
                report.setun_instructions_executed,
                started.elapsed().as_secs_f64(),
                report.immediate_fusions_executed,
                report.branch_fusions_executed,
                report.neg_fusions_executed
            );
            if VERACITY {
                let telemetry = report.telemetry;
                let energy = match telemetry.weighted_energy_reduction_opt() {
                    Some(value) => format!("{value:.2}%"),
                    None => String::from("N/A"),
                };
                println!(
                    "telemetry: trit flips = {} (half {}, full {}) | bit flips = {} | \
                     carry gates = {} | digit reduction = {:.2}% | energy = {}",
                    telemetry.ternary_trit_flips,
                    telemetry.ternary_half_swings,
                    telemetry.ternary_full_swings,
                    telemetry.binary32_bit_flips,
                    telemetry.adder_carry_propagations,
                    telemetry.digit_flip_reduction_pct(),
                    energy
                );
                let carry = telemetry.carry_stats;
                println!(
                    "signed carry: +{} / -{} / zero {} | absorbed {} ({:.1}%) | chains {} | avg {:.2} | max {}",
                    carry.positive,
                    carry.negative,
                    carry.zero,
                    carry.absorbed,
                    carry.absorption_pct(),
                    carry.chains,
                    carry.average_chain(),
                    carry.chain_max
                );
            } else {
                println!("telemetry: disabled in fast mode (no gate simulation)");
            }
        }
        Err(error) => {
            eprintln!(
                "setun-doom rv32: DBT error: {error}\n  guest pc = 0x{:08x} | rv32i = {} | setun = {}",
                engine.pc, engine.rv32i_instructions_executed, engine.setun_instructions_executed
            );
            let tail = &engine.stdout_buffer[engine.stdout_buffer.len().saturating_sub(256)..];
            eprintln!("  guest stdout tail: {}", String::from_utf8_lossy(tail));
        }
    }
}
