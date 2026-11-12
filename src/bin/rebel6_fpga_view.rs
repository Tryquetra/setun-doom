// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Live viewer for the REBEL-6 Doom SoC running on the Tang Nano 20K.
//!
//! Opens `/dev/ttyUSB1` at 3,000,000 baud 8N1 and decodes the UART video stream
//! emitted by the FPGA's background streamer:
//!
//! ```text
//! FE ED 52 36 | frame#lo | frame#hi | palette(768)
//! | escape-byte RLE pixels until 64000 decoded | checksum XOR
//! | \r\n[DOOM FRAME]\r\n
//! ```
//!
//! Keyboard events are sent back as binary packets: `0xFD` (key down) or
//! `0xFC` (key up) followed by the Doom keycode, so the FPGA sees press/release
//! transitions instead of short-lived single characters.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::process::{Command, ExitCode};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use setun_doom::BenchGui;

const WIDTH: usize = 320;
const HEIGHT: usize = 200;
const PIXELS: usize = WIDTH * HEIGHT;
const MAGIC: [u8; 4] = [0xFE, 0xED, 0x52, 0x36];
const TRAILER: [u8; 16] = *b"\r\n[DOOM FRAME]\r\n";
const KEY_DOWN: u8 = 0xFD;
const KEY_UP: u8 = 0xFC;

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

struct LatestFrame {
    palette: Vec<u8>,
    indices: Vec<u8>,
    frame_no: u16,
    frames: u64,
    last: Instant,
    fps: f64,
}

impl Default for LatestFrame {
    fn default() -> Self {
        Self {
            palette: vec![0; 768],
            indices: vec![0; PIXELS],
            frame_no: 0,
            frames: 0,
            last: Instant::now(),
            fps: 0.0,
        }
    }
}

/// Incremental parser for the FPGA video stream.
struct StreamParser {
    buf: Vec<u8>,
    latest: Arc<Mutex<LatestFrame>>,
    printed: u64,
}

impl StreamParser {
    fn new(latest: Arc<Mutex<LatestFrame>>) -> Self {
        Self {
            buf: Vec::new(),
            latest,
            printed: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if byte.is_ascii_graphic() || byte == b'\n' || byte == b'\r' {
            print!("{}", byte as char);
        }
        self.buf.push(byte);
        loop {
            if self.buf.len() < 4 {
                return;
            }
            let Some(pos) = self.buf.windows(4).position(|w| w == MAGIC) else {
                let keep = self.buf.len().min(3);
                self.buf.drain(..self.buf.len() - keep);
                return;
            };
            self.buf.drain(..pos);
            if self.buf.len() < 4 + 2 + 768 + 1 {
                return;
            }
            let mut p = 4usize;
            let frame_no = u16::from_le_bytes([self.buf[4], self.buf[5]]);
            let palette = self.buf[6..6 + 768].to_vec();
            p += 2 + 768;

            let mut indices = Vec::with_capacity(PIXELS);
            let mut cks = 0u8;
            for &b in &self.buf[4..p] {
                cks ^= b;
            }
            // Raw pixel payload: exactly 64000 index bytes.
            while indices.len() < PIXELS && p < self.buf.len() {
                let b = self.buf[p];
                p += 1;
                cks ^= b;
                indices.push(b);
            }
            if indices.len() != PIXELS || p >= self.buf.len() {
                return;
            }
            let checksum = self.buf[p];
            p += 1;
            if cks != checksum {
                self.buf.remove(0);
                continue;
            }
            if p + 16 > self.buf.len() {
                return;
            }
            if self.buf[p..p + 16] != TRAILER {
                self.buf.remove(0);
                continue;
            }
            p += 16;

            let mut latest = self.latest.lock().unwrap();
            latest.palette = palette;
            latest.indices = indices;
            latest.frame_no = frame_no;
            latest.frames += 1;
            let now = Instant::now();
            let dt = now.duration_since(latest.last).as_secs_f64();
            if dt >= 1.0 {
                latest.fps = latest.frames as f64 / dt;
                latest.frames = 0;
                latest.last = now;
            }
            drop(latest);
            self.printed += 1;
            if self.printed % 30 == 0 {
                eprintln!("[viewer] decoded {} live frames", self.printed);
            }
            self.buf.drain(..p);
        }
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut port = String::from("/dev/ttyUSB1");
    let mut baud = String::from("3000000");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().unwrap_or(port),
            "--baud" => baud = args.next().unwrap_or(baud),
            _ => {}
        }
    }

    // Configure the port raw at the requested baud rate.
    match Command::new("stty").args(["-F", &port, &baud, "raw", "-echo"]).status() {
        Ok(status) if status.success() => {}
        other => {
            eprintln!("[viewer] stty configuration failed on {port}: {other:?}");
            eprintln!("[viewer] is the Tang Nano 20K plugged in at {port}?");
            return ExitCode::FAILURE;
        }
    }

    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(0o4000) // O_NONBLOCK
        .open(&port)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[viewer] cannot open {port}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let file = Arc::new(Mutex::new(file));

    let latest = Arc::new(Mutex::new(LatestFrame::default()));
    let reader_latest = latest.clone();
    let reader_file = file.clone();
    let reader = thread::spawn(move || {
        let mut parser = StreamParser::new(reader_latest);
        let mut buf = [0u8; 65536];
        loop {
            let n = {
                let mut f = reader_file.lock().unwrap();
                match f.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        drop(f);
                        thread::sleep(Duration::from_millis(4));
                        continue;
                    }
                    Err(e) => {
                        eprintln!("[viewer] serial read error: {e}");
                        break;
                    }
                }
            };
            // The Sipeed BL616 USB-UART bridge wraps every byte with 0xFA when
            // streaming at 3 Mbaud; de-frame before parsing.
            let mut i = 0usize;
            let mut raw = Vec::with_capacity(n);
            while i < n {
                if buf[i] == 0xFA && i + 1 < n {
                    raw.push(buf[i + 1]);
                    i += 2;
                } else {
                    raw.push(buf[i]);
                    i += 1;
                }
            }
            for &b in &raw {
                parser.push(b);
            }
        }
    });

    let Some(mut gui) = BenchGui::new("REBEL-6 FPGA Doom (3 Mbaud live)") else {
        eprintln!("[viewer] SDL2 init failed");
        return ExitCode::FAILURE;
    };

    let mut rgba = vec![0u8; PIXELS * 4];
    'main: loop {
        // Poll SDL events: send binary key packets to the FPGA.
        let keep = gui.poll_events_desktop(
            |sym, pressed| {
                if let Some(code) = map_key(sym) {
                    let mut f = file.lock().unwrap();
                    let _ = f.write_all(&[if pressed { KEY_DOWN } else { KEY_UP }, code]);
                }
            },
            |_, _| {},
        );
        if !keep {
            break 'main;
        }

        // Render the latest decoded frame.
        {
            let latest = latest.lock().unwrap();
            let pal = &latest.palette;
            for (i, &idx) in latest.indices.iter().enumerate() {
                let pr = idx as usize * 3;
                rgba[i * 4] = pal[pr];
                rgba[i * 4 + 1] = pal[pr + 1];
                rgba[i * 4 + 2] = pal[pr + 2];
                rgba[i * 4 + 3] = 255;
            }
        }
        gui.present_frame(&rgba);
        gui.delay(16);
    }

    // Closing the window tears down the serial thread via process exit.
    let _ = reader.join();
    ExitCode::SUCCESS
}
