//! Runs the doomgeneric guest on the REBEL-6 core with host `ecall` services.
//!
//! Usage: `rebel6-doom <doom_all.tas> [max_frames]`

use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::process::ExitCode;
use std::time::Instant;

use setun_doom::rebel6::asm::assemble;
use setun_doom::rebel6::cpu::{Cpu, Outcome, Trap};
use setun_doom::rebel6::word::Word24;

const SYS_EXIT: i64 = 0;
const SYS_WRITE: i64 = 1;
const SYS_TICKS_MS: i64 = 2;
const SYS_BLIT: i64 = 3;
const SYS_GETKEY: i64 = 4;
const SYS_WAD_OPEN: i64 = 5;
const SYS_WAD_SEEK: i64 = 6;
const SYS_WAD_READ: i64 = 7;
const SYS_WAD_SIZE: i64 = 8;
const SYS_SLEEP_MS: i64 = 9;
const SYS_PALETTE: i64 = 10;

const WIDTH: usize = 320;
const HEIGHT: usize = 200;
const STACK_TOP: i64 = 6_500_000;

struct Host {
    wad: Option<File>,
    wad_len: i64,
    wad_pos: i64,
    palette: Vec<u8>,
    frames: u64,
    max_frames: u64,
    rgba: Vec<u8>,
    started: Instant,
    last_frame: Instant,
    exit_code: Option<i64>,
}

impl Host {
    fn new(max_frames: u64) -> Self {
        Self {
            wad: None,
            wad_len: 0,
            wad_pos: 0,
            palette: vec![0; 768],
            frames: 0,
            max_frames,
            rgba: vec![0; WIDTH * HEIGHT * 4],
            started: Instant::now(),
            last_frame: Instant::now(),
            exit_code: None,
        }
    }

    fn open_wad(&mut self, path: &str) -> bool {
        match File::open(path) {
            Ok(mut file) => {
                self.wad_len = file.seek(SeekFrom::End(0)).unwrap_or(0) as i64;
                self.wad_pos = 0;
                self.wad = Some(file);
                true
            }
            Err(err) => {
                eprintln!("cannot open WAD {path}: {err}");
                false
            }
        }
    }

    fn service(&mut self, cpu: &mut Cpu, num: i64) -> Result<(), Trap> {
        let a0 = cpu.regs[10].to_i32() as i64;
        let a1 = cpu.regs[11].to_i32() as i64;
        let a2 = cpu.regs[12].to_i32() as i64;
        let _ = a2;
        match num {
            SYS_EXIT => {
                self.exit_code = Some(a0);
            }
            SYS_WRITE => {
                let len = a1.max(0) as usize;
                if let Ok(bytes) = cpu.data.read_bytes(a0, len) {
                    print!("{}", String::from_utf8_lossy(&bytes));
                    let _ = std::io::stdout().flush();
                }
                cpu.regs[10] = Word24::from_i32(len as i32);
            }
            SYS_TICKS_MS => {
                // Deterministic clock tied to retired instructions: 1 ms per 1000 instructions.
                let ms = (cpu.cycles / 1000) as i32;
                cpu.regs[10] = Word24::from_i32(ms);
            }
            SYS_BLIT => {
                let len = WIDTH * HEIGHT;
                if let Err(err) = cpu.data.read_bytes(a0, len) {
                    eprintln!("blit: cannot read {len} bytes at {a0}: {err:?}");
                }
                if let Ok(indices) = cpu.data.read_bytes(a0, len) {
                    for (i, &index) in indices.iter().enumerate() {
                        let p = index as usize * 3;
                        self.rgba[i * 4] = self.palette[p];
                        self.rgba[i * 4 + 1] = self.palette[p + 1];
                        self.rgba[i * 4 + 2] = self.palette[p + 2];
                        self.rgba[i * 4 + 3] = 255;
                    }
                    let nonzero = indices.iter().filter(|&&v| v != 0).count();
                    if std::env::var("REBEL6_FRAME_TRACE").is_ok() {
                        eprintln!(
                            "blit frame={} nonzero={} palette0={:?} palette_nonzero={}",
                            self.frames + 1,
                            nonzero,
                            &self.palette[..12],
                            self.palette.iter().filter(|&&v| v != 0).count()
                        );
                    }
                    self.frames += 1;
                    if std::env::var("REBEL6_FRAME_TRACE").is_ok() {
                        let now = Instant::now();
                        eprintln!(
                            "frame={} cycles={} since_start={:?} since_last={:?}",
                            self.frames,
                            cpu.cycles,
                            self.started.elapsed(),
                            now.duration_since(self.last_frame)
                        );
                        self.last_frame = now;
                    }
                    if self.frames <= 3 {
                        let name = format!("rebel6_frame_{}.ppm", self.frames);
                        let _ = save_ppm(&name, &self.rgba);
                        println!("frame {} saved to {name}", self.frames);
                    }
                }
            }
            SYS_GETKEY => {
                cpu.regs[10] = Word24::ZERO;
            }
            SYS_WAD_OPEN => {
                let ok = self.open_wad("vendor/DOOM1.WAD");
                cpu.regs[10] = Word24::from_i32(i32::from(ok));
            }
            SYS_WAD_SEEK => {
                if let Some(file) = self.wad.as_mut() {
                    if file.seek(SeekFrom::Start(a0 as u64)).is_ok() {
                        self.wad_pos = a0;
                    }
                }
                cpu.regs[10] = Word24::ZERO;
            }
            SYS_WAD_READ => {
                let len = a1.max(0) as usize;
                let mut buffer = vec![0u8; len];
                let read = match self.wad.as_mut() {
                    Some(file) => file.read(&mut buffer).unwrap_or(0),
                    None => 0,
                };
                if read > 0 {
                    if std::env::var("REBEL6_WAD_TRACE").is_ok() && self.wad_pos < 128 {
                        eprintln!(
                            "wad read off={} len={} got={} first={:02x?}",
                            self.wad_pos,
                            len,
                            read,
                            &buffer[..read.min(16)]
                        );
                    }
                    let _ = cpu.data.write_bytes(a0, &buffer[..read]);
                    self.wad_pos += read as i64;
                }
                cpu.regs[10] = Word24::from_i32(read as i32);
            }
            SYS_WAD_SIZE => {
                if std::env::var("REBEL6_WAD_TRACE").is_ok() {
                    eprintln!("wad size = {}", self.wad_len);
                }
                cpu.regs[10] = Word24::from_i32(self.wad_len as i32);
            }
            SYS_SLEEP_MS => {}
            SYS_PALETTE => {
                if let Ok(bytes) = cpu.data.read_bytes(a0, 768) {
                    if std::env::var("REBEL6_FRAME_TRACE").is_ok() {
                        eprintln!(
                            "palette call: first={:?} nonzero={}",
                            &bytes[..12],
                            bytes.iter().filter(|&&v| v != 0).count()
                        );
                    }
                    self.palette.copy_from_slice(&bytes);
                }
                cpu.regs[10] = Word24::ZERO;
            }
            other => {
                eprintln!("unknown ecall {other}");
            }
        }
        Ok(())
    }
}

fn save_ppm(path: &str, rgba: &[u8]) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
    for pixel in rgba.chunks(4) {
        file.write_all(&pixel[..3])?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: rebel6-doom <doom_all.tas> [max_frames]");
        return ExitCode::from(2);
    };
    let max_frames = args.get(2).and_then(|s| s.parse::<u64>().ok()).unwrap_or(1);
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("cannot read {path}: {err}");
            return ExitCode::from(1);
        }
    };
    let image = match assemble(&source) {
        Ok(image) => image,
        Err(err) => {
            eprintln!("assembly failed: {err}");
            return ExitCode::from(1);
        }
    };
    println!(
        "image: {} instructions, {} data bytes, entry {}, data base {}",
        image.instrs.len(),
        image.data.len(),
        image.entry,
        image.data_base
    );
    for name in ["_heap_start", "_heap_end", "_start", "main"] {
        println!("{name} = {:?}", image.symbols.get(name));
    }
    let mut cpu = image.machine();
    cpu.write_reg(2, Word24::new(STACK_TOP)).unwrap();
    cpu.write_reg(1, Word24::new(-1)).unwrap();
    let mut host = Host::new(max_frames);
    let start = Instant::now();
    let mut steps: u64 = 0;
    loop {
        if host.exit_code.is_some() {
            break;
        }
        if host.frames >= host.max_frames {
            println!("reached frame limit");
            break;
        }
        match cpu.step() {
            Ok(Outcome::Continue) => {}
            Ok(Outcome::Ecall(num)) => {
                if let Err(trap) = host.service(&mut cpu, num) {
                    eprintln!("ecall trap: {trap:?}");
                    return ExitCode::from(4);
                }
            }
            Ok(Outcome::Halt(code)) => {
                println!("halt {code}");
                break;
            }
            Err(Trap::StepLimit) => break,
            Err(trap) => {
                let pc = cpu.pc;
                if let Some(word) = cpu.instrs.get(pc as usize) {
                    if let Some(instr) = setun_doom::rebel6::encoding::decode(*word) {
                        eprintln!("faulting instruction: {:?}", instr);
                    } else {
                        eprintln!("faulting word: {word:#x}");
                    }
                }
                eprintln!(
                    "regs: ra={} sp={} a0={} a1={} a2={} a3={} s0={} s1={}",
                    cpu.regs[1],
                    cpu.regs[2],
                    cpu.regs[10],
                    cpu.regs[11],
                    cpu.regs[12],
                    cpu.regs[13],
                    cpu.regs[8],
                    cpu.regs[9]
                );
                let mut nearest: Option<(&str, i64)> = None;
                for (name, &addr) in &image.symbols {
                    if addr <= pc && addr >= 0 && nearest.is_none_or(|(_, best)| addr > best) {
                        nearest = Some((name, addr));
                    }
                }
                eprintln!("nearest symbol: {:?}", nearest);
                for i in (pc - 8).max(0)..=(pc + 12).min(cpu.instrs.len() as i64 - 1) {
                    if let Some(instr) =
                        setun_doom::rebel6::encoding::decode(cpu.instrs[i as usize])
                    {
                        eprintln!("  [{i}] {:?}", instr);
                    }
                }
                eprintln!("trap at pc={} cycles={}: {trap:?}", pc, cpu.cycles);
                return ExitCode::from(4);
            }
        }
        steps += 1;
        if std::env::var("REBEL6_TRACE").is_ok() && steps.is_multiple_of(1_000_000) {
            eprintln!(
                "step={} pc={} sp={} ra={}",
                steps, cpu.pc, cpu.regs[2], cpu.regs[1]
            );
        }
        if steps.is_multiple_of(100_000_000) {
            println!("steps={steps} pc={} frames={}", cpu.pc, host.frames);
        }
        if start.elapsed().as_secs() > 120 {
            println!("wall-clock limit");
            break;
        }
    }
    println!(
        "done: cycles={} trit_flips={} frames={} elapsed={:?}",
        cpu.cycles,
        cpu.trit_flips,
        host.frames,
        start.elapsed()
    );
    ExitCode::SUCCESS
}
