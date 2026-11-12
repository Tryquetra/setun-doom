// Cycle-accurate Verilator simulation of the complete Tang Nano 20K REBEL-6 SoC:
//   - Models the 8 MiB XT25F64B SPI NOR Flash (power-down wake 0xAB + 0x03 burst read)
//     populated at 0x100000 with doom_boot.bin and at 0x200000 with DOOM1.WAD
//   - Decodes live UART TX output from the SoC
//   - Captures a full 640x480@60Hz HDMI scanout frame once target_frames is reached
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include "Vrebel6_soc_sim_tb.h"
#include "verilated.h"

#include <fcntl.h>
#include <unistd.h>

struct SpiFlashModel {
    std::vector<uint8_t> mem;
    bool     awake     = false;
    bool     prev_cs_n = true;
    bool     prev_clk  = false;
    uint32_t bit_idx   = 0;
    uint8_t  cmd       = 0;
    uint32_t addr      = 0;
    uint8_t  cur_out   = 0xFF;

    SpiFlashModel() : mem(8 * 1024 * 1024, 0xFF) {}

    void load_file(const std::string& path, uint32_t offset) {
        std::ifstream f(path, std::ios::binary);
        if (!f) {
            std::cerr << "cannot open flash payload: " << path << "\n";
            std::exit(1);
        }
        f.seekg(0, std::ios::end);
        size_t sz = f.tellg();
        f.seekg(0, std::ios::beg);
        if (offset + sz > mem.size()) {
            std::cerr << "payload overflows 8 MiB flash: " << path << "\n";
            std::exit(1);
        }
        f.read(reinterpret_cast<char*>(mem.data() + offset), sz);
    }

    uint8_t step(bool cs_n, bool sclk, bool mosi) {
        if (cs_n) {
            prev_cs_n = true;
            prev_clk  = sclk;
            return 1;
        }
        if (prev_cs_n && !cs_n) {
            bit_idx = 0;
            cmd     = 0;
            addr    = 0;
            cur_out = 0xFF;
        }
        prev_cs_n = false;

        // Rising edge: sample MOSI
        if (!prev_clk && sclk) {
            if (bit_idx < 8) {
                cmd = static_cast<uint8_t>((cmd << 1) | (mosi ? 1 : 0));
                if (bit_idx == 7 && cmd == 0xAB) {
                    awake = true;
                }
            } else if (bit_idx < 32) {
                addr = (addr << 1) | (mosi ? 1U : 0U);
            }
            bit_idx++;
        }

        // Falling edge: shift out MISO for 0x03 Read Data once 32 cmd+addr bits received
        if (prev_clk && !sclk) {
            if (awake && cmd == 0x03 && bit_idx >= 32) {
                uint32_t data_bit = bit_idx - 32;
                if ((data_bit & 7U) == 0U) {
                    uint32_t byte_off = (addr + (data_bit >> 3)) & 0x7FFFFFU;
                    cur_out = mem[byte_off];
                } else {
                    cur_out <<= 1;
                }
            }
        }

        prev_clk = sclk;
        return (cur_out >> 7) & 1U;
    }
};

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    const std::string boot_bin = (argc >= 2) ? argv[1] : "fpga/build/doom_boot.bin";
    const std::string wad_bin  = (argc >= 3) ? argv[2] : "vendor/DOOM1.WAD";
    const uint32_t    target_frames = (argc >= 4) ? std::strtoul(argv[3], nullptr, 10) : 4;
    const std::string out_ppm  = (argc >= 5) ? argv[4] : "fpga/build/soc/soc_hdmi_640x480.ppm";

    SpiFlashModel flash;
    flash.load_file(boot_bin, 0x100000);
    if (!wad_bin.empty() && wad_bin != "NONE") {
        flash.load_file(wad_bin, 0x200000);
    }

    Vrebel6_soc_sim_tb tb;
    tb.clk        = 0;
    tb.btn        = 0;
    tb.uart_rx    = 1;
    tb.flash_miso = 1;

    // UART receiver (SIM_ONLY uses DIV=2 -> 2 clocks/bit)
    int      uart_state = 0;
    int      uart_cnt   = 0;
    int      uart_bits  = 0;
    uint8_t  uart_sh    = 0;
    uint64_t cyc = 0;
    uint16_t last_frames = 0;

    // Live stream frame parser: FE ED 52 36 | frame#lo | frame#hi | palette(768)
    // | escape-byte RLE pixels (64000) | checksum XOR | \r\n[DOOM FRAME]\r\n
    static const uint8_t kMagic[4] = {0xFE, 0xED, 0x52, 0x36};
    static const char kTrailer[17] = "\r\n[DOOM FRAME]\r\n";
    std::vector<uint8_t> ubuf;
    uint32_t stream_frames = 0;
    size_t scan_pos = 0;          // magic search watermark (SYNC state)
    uint64_t magic_hits = 0;
    uint64_t cks_mismatch = 0;
    uint64_t rle_mismatch = 0;
    uint64_t trail_mismatch = 0;
    bool debug_once = true;
    int rawfd = ::open("/tmp/sim_raw.bin", O_WRONLY | O_CREAT | O_TRUNC, 0644);

    // Persistent frame-decode state: 0=SYNC,1=MAGIC,2=HDR,3=PAL,4=RLE,5=CKSUM,6=TRAILER
    int fstate = 0;
    size_t fpos = 0;
    uint16_t fno = 0;
    uint8_t fpal[768];
    uint8_t fcks = 0;
    std::vector<uint8_t> fidx;

    auto stream_push = [&](uint8_t b) {
        if (b >= 0x20 && b < 0x7f) {
            std::putchar(b);
            std::fflush(stdout);
        }
        if (rawfd >= 0) {
            ssize_t w = ::write(rawfd, &b, 1);
            (void)w;
        }
        ubuf.push_back(b);

        while (true) {
            if (fstate == 0) {
                // SYNC: find the next frame magic.
                if (ubuf.size() < scan_pos + 4) return;
                auto it = std::search(ubuf.begin() + scan_pos, ubuf.end(), kMagic, kMagic + 4);
                scan_pos = static_cast<size_t>(it - ubuf.begin());
                if (it == ubuf.end()) {
                    if (ubuf.size() > 3) {
                        ubuf.erase(ubuf.begin(), ubuf.end() - 3);
                        scan_pos = 0;
                    }
                    return;
                }
                magic_hits++;
                if (scan_pos > 0) {
                    ubuf.erase(ubuf.begin(), ubuf.begin() + scan_pos);
                    scan_pos = 0;
                }
                fstate = 1;
                fpos = 4;
                fcks = 0;
                fidx.clear();
                fidx.reserve(64000);
                continue;
            }

            if (fstate == 1) { // MAGIC done; expect frame#(2)
                if (ubuf.size() < fpos + 2) return;
                fno = static_cast<uint16_t>(ubuf[fpos] | (ubuf[fpos + 1] << 8));
                fcks ^= ubuf[fpos] ^ ubuf[fpos + 1];
                fpos += 2;
                fstate = 2;
                continue;
            }
            if (fstate == 2) { // HDR done; expect palette(768)
                if (ubuf.size() < fpos + 768) return;
                std::copy_n(ubuf.begin() + fpos, 768, fpal);
                for (size_t i = 0; i < 768; i++) fcks ^= fpal[i];
                fpos += 768;
                fstate = 3;
                continue;
            }
            if (fstate == 3) { // raw 64000 pixel bytes
                while (fidx.size() < 64000 && fpos < ubuf.size()) {
                    uint8_t b = ubuf[fpos++];
                    fcks ^= b;
                    fidx.push_back(b);
                }
                if (fidx.size() != 64000) return; // wait for more pixels
                fstate = 4;
                continue;
            }
            if (fstate == 4) { // checksum byte
                if (ubuf.size() < fpos + 1) return;
                uint8_t stored = ubuf[fpos++];
                if (fcks != stored) {
                    if (debug_once) {
                        debug_once = false;
                        std::cerr << "[sim_soc] CKSUM fail: fno=" << fno
                                  << " idx=" << fidx.size()
                                  << " cks=" << static_cast<int>(fcks)
                                  << " stored=" << static_cast<int>(stored)
                                  << " pos=" << fpos << " bufsz=" << ubuf.size() << "\n";
                    }
                    cks_mismatch++;
                    fstate = 0;
                    scan_pos = 1;
                    ubuf.erase(ubuf.begin());
                    fidx.clear();
                    continue;
                }
                fstate = 5;
                continue;
            }
            if (fstate == 5) { // trailer
                if (ubuf.size() < fpos + 16 + 2) return; // trailer + pixel-count footer
                if (memcmp(ubuf.data() + fpos, kTrailer, 16) != 0) {
                    if (debug_once) {
                        debug_once = false;
                        std::cerr << "[sim_soc] TRAILER fail: fno=" << fno
                                  << " pos=" << fpos << " bufsz=" << ubuf.size() << "\n";
                    }
                    trail_mismatch++;
                    fstate = 0;
                    scan_pos = 1;
                    ubuf.erase(ubuf.begin());
                    fidx.clear();
                    continue;
                }
                uint16_t enc_pix = static_cast<uint16_t>(ubuf[fpos + 16] | (ubuf[fpos + 17] << 8));
                if (enc_pix != 64000) {
                    if (debug_once) {
                        debug_once = false;
                        std::cerr << "[sim_soc] PIXCOUNT fail: fno=" << fno
                                  << " enc_pix=" << enc_pix
                                  << " idx=" << fidx.size() << " bufsz=" << ubuf.size() << "\n";
                    }
                    fstate = 0;
                    scan_pos = 1;
                    ubuf.erase(ubuf.begin());
                    fidx.clear();
                    continue;
                }
                size_t consumed = fpos + 18;
                stream_frames++;
                std::cerr << "\n[sim_soc] stream frame " << stream_frames
                          << " (#" << fno << ", RLE " << (consumed - (6 + 768 + 2))
                          << "B, xor OK, pix=" << enc_pix
                          << ") at soc_clk=" << cyc << "\n";
                if (stream_frames <= 3) {
                    std::string fn = "fpga/build/soc/soc_uart_frame_"
                                   + std::to_string(stream_frames) + ".ppm";
                    std::ofstream out(fn, std::ios::binary);
                    out << "P6\n320 200\n255\n";
                    for (uint8_t i : fidx) {
                        out.put(fpal[i * 3]);
                        out.put(fpal[i * 3 + 1]);
                        out.put(fpal[i * 3 + 2]);
                    }
                    std::cerr << "[sim_soc] saved decoded UART frame " << fn << "\n";
                }
                ubuf.erase(ubuf.begin(), ubuf.begin() + consumed);
                fstate = 0;
                scan_pos = 0;
                fidx.clear();
                if (stream_frames >= target_frames) {
                    std::cerr << "[sim_soc] decode counters: magic_hits=" << magic_hits
                              << " cks_mismatch=" << cks_mismatch
                              << " rle_mismatch=" << rle_mismatch
                              << " trail_mismatch=" << trail_mismatch << "\n";
                    if (rawfd >= 0) { ::close(rawfd); rawfd = -1; }
                    return;
                }
                continue;
            }
            return;
        }
    };

    while (!tb.dbg_trapped && !tb.dbg_boot_err) {
        tb.clk = 0;
        tb.eval();
        tb.flash_miso = flash.step(tb.flash_cs_n, tb.flash_clk, tb.flash_mosi);
        tb.eval();

        tb.clk = 1;
        tb.eval();
        tb.flash_miso = flash.step(tb.flash_cs_n, tb.flash_clk, tb.flash_mosi);
        tb.eval();

        // Decode SIM_ONLY UART TX (DIV = 2 clocks per bit)
        if (uart_state == 0) {
            if (!tb.uart_tx) {
                uart_state = 1;
                uart_cnt   = 2;
                uart_bits  = 0;
                uart_sh    = 0;
            }
        } else {
            uart_cnt--;
            if (uart_cnt == 0) {
                if (uart_bits < 8) {
                    uart_sh = static_cast<uint8_t>((uart_sh >> 1) | (tb.uart_tx ? 0x80 : 0));
                    uart_bits++;
                    uart_cnt = 2;
                } else {
                    stream_push(uart_sh);
                    uart_state = 0;
                }
            }
        }

        if (tb.dbg_frames != last_frames) {
            last_frames = tb.dbg_frames;
            std::cerr << "\n[sim_soc] reached frame " << last_frames << " at soc_clk=" << cyc << "\n";
        }
        cyc++;
    }

    if (tb.dbg_trapped || tb.dbg_boot_err) {
        std::cerr << "FAIL: trapped=" << static_cast<int>(tb.dbg_trapped)
                  << " boot_err=" << static_cast<int>(tb.dbg_boot_err)
                  << " at clk=" << cyc << "\n";
        return 1;
    }

    // Capture one complete 640x480@60Hz HDMI scanout frame (sync to falling VSYNC -> active DE)
    std::cerr << "[sim_soc] capturing 640x480 HDMI scanout frame...\n";
    bool prev_vs = tb.hdmi_vs;
    while (true) {
        tb.clk = 0; tb.eval(); tb.clk = 1; tb.eval();
        if (prev_vs && !tb.hdmi_vs) break; // start of VSYNC pulse
        prev_vs = tb.hdmi_vs;
    }
    while (!tb.hdmi_vs) {
        tb.clk = 0; tb.eval(); tb.clk = 1; tb.eval();
    }

    std::vector<uint8_t> rgb;
    rgb.reserve(640 * 480 * 3);
    while (rgb.size() < 640 * 480 * 3) {
        tb.clk = 0; tb.eval(); tb.clk = 1; tb.eval();
        if (tb.hdmi_de) {
            rgb.push_back(tb.hdmi_r);
            rgb.push_back(tb.hdmi_g);
            rgb.push_back(tb.hdmi_b);
        }
    }

    std::ofstream ppm(out_ppm, std::ios::binary);
    std::string hdr = "P6\n640 480\n255\n";
    ppm.write(hdr.data(), hdr.size());
    ppm.write(reinterpret_cast<const char*>(rgb.data()), rgb.size());
    std::cerr << "[sim_soc] wrote 640x480 HDMI frame to " << out_ppm << "\n";
    return 0;
}
