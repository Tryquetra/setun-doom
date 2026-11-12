#!/usr/bin/env python3
"""Program SPI Flash payloads + REBEL-6 Doom SoC bitstream on Tang Nano 20K
and capture live UART console + [DOOM FRAME] telemetry over /dev/ttyUSB1.
"""
from __future__ import annotations

import argparse
import os
import select
import subprocess
import sys
import termios
import time


def open_uart(port: str, baud: int) -> int:
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    attrs = termios.tcgetattr(fd)
    baud_map = {
        115200: termios.B115200,
        230400: termios.B230400,
        460800: termios.B460800,
        921600: termios.B921600,
        2000000: termios.B2000000,
        3000000: termios.B3000000,
    }
    bconst = baud_map[baud]
    attrs[0] = 0
    attrs[1] = 0
    attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
    attrs[3] = 0
    attrs[4] = bconst
    attrs[5] = bconst
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    termios.tcflush(fd, termios.TCIOFLUSH)
    return fd


def run_cmd(cmd: list[str]) -> None:
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, check=True)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--bitstream", default="fpga/build/soc/rebel6_doom_soc.fs")
    ap.add_argument("--boot-img", default="fpga/build/doom_boot.bin")
    ap.add_argument("--wad", default="vendor/DOOM1.WAD")
    ap.add_argument("--skip-wad", action="store_true")
    ap.add_argument("--skip-boot", action="store_true")
    ap.add_argument("--flash-bitstream", action="store_true")
    ap.add_argument("--uart", default="/dev/ttyUSB1")
    ap.add_argument("--baud", type=int, default=3000000)
    ap.add_argument("--target-frames", type=int, default=6)
    ap.add_argument("--timeout", type=float, default=60.0)
    args = ap.parse_args()

    if not args.skip_boot:
        run_cmd([
            "openFPGALoader", "-b", "tangnano20k",
            "-f", "-o", "0x100000", args.boot_img,
        ])

    if not args.skip_wad:
        run_cmd([
            "openFPGALoader", "-b", "tangnano20k",
            "-f", "-o", "0x200000", args.wad,
        ])

    # Open UART BEFORE loading bitstream so we catch the very first boot byte!
    fd = open_uart(args.uart, args.baud)

    if args.flash_bitstream:
        run_cmd(["openFPGALoader", "-b", "tangnano20k", "-f", args.bitstream])
    else:
        run_cmd(["openFPGALoader", "-b", "tangnano20k", args.bitstream])

    print(f"\n--- Monitoring {args.uart} @ {args.baud} 8N1 (waiting for {args.target_frames} HDMI frames) ---", flush=True)
    buf = bytearray()
    frames = 0
    t0 = time.monotonic()

    try:
        while (time.monotonic() - t0) < args.timeout:
            r, _, _ = select.select([fd], [], [], 0.2)
            if r:
                chunk = os.read(fd, 4096)
                if chunk:
                    buf.extend(chunk)
                    text = chunk.decode("latin1", errors="replace")
                    sys.stdout.write(text)
                    sys.stdout.flush()
                    frames = buf.count(b"[DOOM FRAME]")
                    if frames >= args.target_frames:
                        elapsed = time.monotonic() - t0
                        print(f"\n--- Verified {frames} live Doom frames on physical FPGA in {elapsed:.2f}s! ---", flush=True)
                        return 0
    finally:
        os.close(fd)

    print(f"\n--- Timeout after {args.timeout:.1f}s (captured {frames} frames, {len(buf)} UART bytes) ---", flush=True)
    return 1 if frames < args.target_frames else 0


if __name__ == "__main__":
    raise SystemExit(main())
