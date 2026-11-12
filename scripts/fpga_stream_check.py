#!/usr/bin/env python3
"""Physical verification of the REBEL-6 Doom live UART stream.

Opens /dev/ttyUSB1 (or arg) at 3,000,000 baud 8N1, decodes the stream packet
(FE ED 52 36 | frame# | palette(768) | raw 64000 pixel bytes | checksum |
\r\n[DOOM FRAME]\r\n | pix count lo/hi) and reports per-frame stats.

The Sipeed BL616 USB-UART bridge wraps every FPGA->PC byte with 0xFA; the
deframer (default) strips those headers. Use --raw-fa to disable stripping.
"""
import os
import select
import sys
import termios
import time

PORT = sys.argv[1] if len(sys.argv) > 1 else "/dev/ttyUSB1"
BAUD = 3_000_000
STRIP_FA = "--raw-fa" not in sys.argv
MAGIC = bytes([0xFE, 0xED, 0x52, 0x36])
TRAILER = b"\r\n[DOOM FRAME]\r\n"
W, H = 320, 200
PIXELS = W * H


def deframe(b):
    out = bytearray()
    i = 0
    while i < len(b):
        if b[i] == 0xFA and i + 1 < len(b):
            out.append(b[i + 1])
            i += 2
        else:
            out.append(b[i])
            i += 1
    return bytes(out)


def open_uart(port: str, baud: int) -> int:
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    attrs = termios.tcgetattr(fd)
    bmap = {115200: termios.B115200, 921600: termios.B921600,
            1000000: termios.B1000000, 2000000: termios.B2000000,
            3000000: termios.B3000000}
    attrs[0] = 0
    attrs[1] = 0
    attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
    attrs[3] = 0
    attrs[4] = bmap[baud]
    attrs[5] = bmap[baud]
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    termios.tcflush(fd, termios.TCIOFLUSH)
    return fd


def decode(buf, pos):
    """Try to decode one frame at buf[pos:].
    Returns (consumed, palette, indices, frame_no, pix_count) or None."""
    p = pos + 4
    if len(buf) < p + 2 + 768 + 1:
        return None
    frame_no = buf[p] | (buf[p + 1] << 8)
    palette = buf[p + 2:p + 2 + 768]
    p += 2 + 768
    indices = bytearray()
    cks = 0
    for b in buf[pos + 4:p]:
        cks ^= b
    while len(indices) < PIXELS and p < len(buf):
        b = buf[p]
        p += 1
        cks ^= b
        indices.append(b)
    if len(indices) != PIXELS or p >= len(buf):
        return None
    if cks != buf[p]:
        return ("badcksum",)
    p += 1
    if p + 16 + 2 > len(buf):
        return None
    if buf[p:p + 16] != TRAILER:
        return ("badtrailer",)
    p += 16
    pix_count = buf[p] | (buf[p + 1] << 8)
    p += 2
    return p, palette, bytes(indices), frame_no, pix_count


def main():
    fd = open_uart(PORT, BAUD)
    buf = bytearray()
    frames = 0
    fps = 0.0
    t0 = time.monotonic()
    t_last = t0
    print(f"Monitoring {PORT} @ {BAUD} 8N1 ...")
    while True:
        r, _, _ = select.select([fd], [], [], 0.25)
        if r:
            chunk = os.read(fd, 65536)
            if not chunk:
                time.sleep(0.01)
                continue
            if STRIP_FA:
                chunk = deframe(chunk)
            printable = bytes(c for c in chunk if 0x20 <= c < 0x7f or c in (10, 13))
            if printable:
                sys.stdout.write(printable.decode("latin1", errors="replace"))
                sys.stdout.flush()
            buf.extend(chunk)
        while True:
            idx = buf.find(MAGIC)
            if idx < 0:
                if len(buf) > 3:
                    del buf[:-3]
                break
            if idx > 0:
                del buf[:idx]
            res = decode(buf, 0)
            if res is None:
                break
            if res[0] in ("badcksum", "badtrailer"):
                del buf[0]
                continue
            consumed, palette, indices, fno, pix_count = res
            del buf[:consumed]
            frames += 1
            now = time.monotonic()
            dt = now - t_last
            if dt >= 1.0:
                fps = frames / dt
                frames = 0
                t_last = now
            total = int(now - t0)
            ok = "OK" if pix_count == PIXELS else f"PIX={pix_count}"
            print(f"\n[fpga] frame #{fno} decoded @ {total}s ({fps:.2f} fps) "
                  f"payload={consumed - 4 - 2 - 768 - 1 - 16 - 2}B {ok}",
                  flush=True)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nstopped")
