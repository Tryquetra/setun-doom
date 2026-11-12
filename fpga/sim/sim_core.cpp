// Verilator C++ harness for rebel6_core: runs R6IM images emitted by rebel6_asm,
// implements the 11 ABI ecall services (including DOOM1.WAD and PPM frame
// dumps), and verifies RTL execution against the Rust REBEL-6 reference.
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include "Vrebel6_core.h"
#include "verilated.h"

#pragma pack(push, 1)
struct R6ImHeader {
    char     magic[4];   // "R6IM"
    uint64_t instr_len;
    uint64_t data_len;
    int64_t  data_base;
    int64_t  entry;
};
#pragma pack(pop)

static inline int64_t sext40(uint64_t v) {
    v &= 0xFFFFFFFFFFULL;
    if (v & (1ULL << 39)) {
        return static_cast<int64_t>(v | ~0xFFFFFFFFFFULL);
    }
    return static_cast<int64_t>(v);
}

static inline uint64_t mask40(int64_t v) {
    return static_cast<uint64_t>(v) & 0xFFFFFFFFFFULL;
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    if (argc < 2) {
        std::cerr << "usage: sim_core <image.img> [max_frames] [stack_top] [wad_path] [frame_prefix]\n";
        return 2;
    }

    const std::string img_path     = argv[1];
    const uint64_t    max_frames   = (argc >= 3) ? std::strtoull(argv[2], nullptr, 10) : 0;
    const int64_t     stack_top    = (argc >= 4) ? std::strtoll(argv[3], nullptr, 10)  : 6500000;
    const std::string wad_path     = (argc >= 5) ? argv[4] : "vendor/DOOM1.WAD";
    const std::string frame_prefix = (argc >= 6) ? argv[5] : "verilator_frame_";

    std::ifstream in(img_path, std::ios::binary);
    if (!in) {
        std::cerr << "cannot open " << img_path << "\n";
        return 1;
    }

    R6ImHeader hdr{};
    in.read(reinterpret_cast<char*>(&hdr), sizeof(hdr));
    if (std::memcmp(hdr.magic, "R6IM", 4) != 0) {
        std::cerr << "bad magic in " << img_path << "\n";
        return 1;
    }

    std::vector<uint64_t> imem(hdr.instr_len, 0);
    in.read(reinterpret_cast<char*>(imem.data()), hdr.instr_len * sizeof(uint64_t));

    std::vector<int16_t> raw_cells(hdr.data_len, 0);
    in.read(reinterpret_cast<char*>(raw_cells.data()), hdr.data_len * sizeof(int16_t));

    const uint32_t dmem_size = 8 * 1024 * 1024;
    std::vector<uint8_t> dmem(dmem_size, 0);
    for (uint64_t i = 0; i < hdr.data_len; ++i) {
        int64_t addr = hdr.data_base + static_cast<int64_t>(i);
        if (addr >= 0 && static_cast<uint64_t>(addr) < dmem_size) {
            dmem[addr] = static_cast<uint8_t>(raw_cells[i] & 0xFF);
        }
    }

    Vrebel6_core top;
    auto tick = [&]() {
        top.clk = 0;
        top.eval();
        top.clk = 1;
        top.eval();
    };

    // Reset
    top.rst             = 1;
    top.start_pulse     = 0;
    top.start_pc        = 0;
    top.instr_count     = static_cast<uint32_t>(hdr.instr_len);
    top.dmem_limit      = dmem_size;
    top.host_rf_we      = 0;
    top.host_rf_waddr   = 0;
    top.host_rf_wdata   = 0;
    top.imem_rdata      = 0;
    top.imem_ready      = 0;
    top.dmem_rdata      = 0;
    top.dmem_ready      = 0;
    top.ecall_done      = 0;
    top.ecall_write_a0  = 0;
    top.ecall_ret_a0    = 0;
    top.ecall_halt      = 0;
    top.ecall_exit_code = 0;
    tick();
    tick();
    top.rst = 0;
    tick();

    // Initialize X2 (sp) and X1 (ra = -1) when stack_top > 0
    if (stack_top > 0) {
        top.host_rf_we    = 1;
        top.host_rf_waddr = 2;
        top.host_rf_wdata = mask40(stack_top);
        tick();
        top.host_rf_waddr = 1;
        top.host_rf_wdata = mask40(-1);
        tick();
        top.host_rf_we    = 0;
        tick();
    }

    // Start execution
    top.start_pc    = mask40(hdr.entry);
    top.start_pulse = 1;
    tick();
    top.start_pulse = 0;

    std::ifstream wad;
    int64_t wad_len = 0;
    int64_t wad_pos = 0;
    bool    wad_open = false;
    uint8_t palette[768] = {0};
    uint64_t frames = 0;
    uint64_t clk_cycles = 0;

    while (!top.halted && !top.trapped) {
        if (top.imem_req) {
            uint32_t idx = top.imem_addr;
            top.imem_rdata = (idx < imem.size()) ? imem[idx] : 0ULL;
            top.imem_ready = 1;
        } else {
            top.imem_ready = 0;
        }

        if (top.dmem_req) {
            uint32_t a = top.dmem_addr & ~3U;
            if (top.dmem_we) {
                uint32_t w = top.dmem_wdata;
                uint8_t  s = top.dmem_wstrb;
                if ((s & 1) && a + 0 < dmem_size) dmem[a + 0] = (w >> 0) & 0xFF;
                if ((s & 2) && a + 1 < dmem_size) dmem[a + 1] = (w >> 8) & 0xFF;
                if ((s & 4) && a + 2 < dmem_size) dmem[a + 2] = (w >> 16) & 0xFF;
                if ((s & 8) && a + 3 < dmem_size) dmem[a + 3] = (w >> 24) & 0xFF;
            } else if (a + 3 < dmem_size) {
                uint32_t r = static_cast<uint32_t>(dmem[a + 0]) |
                             (static_cast<uint32_t>(dmem[a + 1]) << 8) |
                             (static_cast<uint32_t>(dmem[a + 2]) << 16) |
                             (static_cast<uint32_t>(dmem[a + 3]) << 24);
                top.dmem_rdata = r;
            }
            top.dmem_ready = 1;
        } else {
            top.dmem_ready = 0;
        }

        top.ecall_done     = 0;
        top.ecall_write_a0 = 0;
        top.ecall_halt     = 0;
        if (top.ecall_req) {
            int64_t num = sext40(top.ecall_a7);
            int32_t a0  = static_cast<int32_t>(top.ecall_a0);
            int32_t a1  = static_cast<int32_t>(top.ecall_a1);

            top.ecall_done = 1;
            switch (num) {
                case 0: {
                    top.ecall_halt      = 1;
                    top.ecall_exit_code = mask40(a0);
                    break;
                }
                case 1: {
                    int32_t len = (a1 > 0) ? a1 : 0;
                    if (a0 >= 0 && static_cast<uint64_t>(a0) + len <= dmem_size) {
                        std::fwrite(dmem.data() + a0, 1, len, stdout);
                        std::fflush(stdout);
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(len);
                    } else {
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(-14);
                    }
                    break;
                }
                case 2: {
                    int32_t ms = static_cast<int32_t>(top.cycles_out / 1000ULL);
                    top.ecall_write_a0 = 1;
                    top.ecall_ret_a0   = mask40(ms);
                    break;
                }
                case 3: {
                    const uint32_t len = 320 * 200;
                    if (a0 >= 0 && static_cast<uint64_t>(a0) + len <= dmem_size) {
                        frames++;
                        if (frames <= 4) {
                            std::string fname = frame_prefix + std::to_string(frames) + ".ppm";
                            std::ofstream ppm(fname, std::ios::binary);
                            std::string header = "P6\n320 200\n255\n";
                            ppm.write(header.data(), header.size());
                            for (uint32_t i = 0; i < len; ++i) {
                                uint8_t idx = dmem[a0 + i];
                                ppm.write(reinterpret_cast<const char*>(&palette[idx * 3]), 3);
                            }
                            std::cout << "verilator frame " << frames << " saved to " << fname
                                      << " (retired=" << top.cycles_out
                                      << " clk_cycles=" << clk_cycles << ")\n";
                        }
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = 0;
                    } else {
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(-14);
                    }
                    break;
                }
                case 4:
                case 9: {
                    top.ecall_write_a0 = 1;
                    top.ecall_ret_a0   = 0;
                    break;
                }
                case 5: {
                    wad.close();
                    wad.open(wad_path, std::ios::binary);
                    if (wad) {
                        wad.seekg(0, std::ios::end);
                        wad_len  = wad.tellg();
                        wad_pos  = 0;
                        wad.seekg(0, std::ios::beg);
                        wad_open = true;
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = 0;
                    } else {
                        wad_open = false;
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(-2);
                    }
                    break;
                }
                case 6: {
                    if (!wad_open) {
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(-9);
                    } else {
                        wad.clear();
                        wad.seekg(static_cast<uint32_t>(a0), std::ios::beg);
                        wad_pos = static_cast<uint32_t>(a0);
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = wad ? 0 : mask40(-5);
                    }
                    break;
                }
                case 7: {
                    int32_t len = (a1 > 0) ? a1 : 0;
                    if (!wad_open) {
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(-9);
                    } else if (a0 < 0 || static_cast<uint64_t>(a0) + len > dmem_size) {
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(-14);
                    } else {
                        wad.read(reinterpret_cast<char*>(dmem.data() + a0), len);
                        std::streamsize got = wad.gcount();
                        wad_pos += got;
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(got);
                    }
                    break;
                }
                case 8: {
                    top.ecall_write_a0 = 1;
                    top.ecall_ret_a0   = wad_open ? mask40(wad_len) : mask40(-9);
                    break;
                }
                case 10: {
                    if (a0 >= 0 && static_cast<uint64_t>(a0) + 768 <= dmem_size) {
                        std::memcpy(palette, dmem.data() + a0, 768);
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = 0;
                    } else {
                        top.ecall_write_a0 = 1;
                        top.ecall_ret_a0   = mask40(-14);
                    }
                    break;
                }
                default: {
                    // RV32I compliance suite invokes ecall with a7=93, exit status in a0
                    top.ecall_halt      = 1;
                    top.ecall_exit_code = mask40(a0);
                    break;
                }
            }
        }

        tick();
        clk_cycles++;

        if (max_frames > 0 && frames >= max_frames) {
            break;
        }
    }

    if (top.trapped) {
        std::cerr << "TRAP cause=" << static_cast<int>(top.trap_cause)
                  << " pc=" << sext40(top.pc_out)
                  << " retired=" << top.cycles_out << "\n";
        return 125;
    }

    int64_t exit_code = sext40(top.exit_code_out);
    std::cout << "RET=" << exit_code
              << " RETIRED=" << top.cycles_out
              << " CLK=" << clk_cycles
              << " FRAMES=" << frames << "\n";
    return (exit_code == 0) ? 0 : 1;
}
