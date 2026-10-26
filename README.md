# Setun-Doom: 21-Trit Balanced Ternary Computer Emulator Running Doom (1993)

![Setun-Doom E1M1 Render](assets/setun_doom_e1m1.png)

**Setun-Doom** is a gate-level balanced ternary computer architecture emulator (`{-1, 0, +1}`) written in Rust that executes classic **Doom (1993)** (`DOOM1.WAD` via `doomgeneric`). It synthesizes historical Soviet balanced ternary architectures (**Setun-1958** and **Setun-70** by Nikolai Brusentsov, Moscow State University) with modern ternary RISC extensions (**REBEL-2** and **REBEL-6**, IEEE ISMVL 2025) and **J. Ramil Alvarez's (1974)** recurrent $p \to p \pm 1$ hardware base converter.

---

## Architectural Highlights

* **21-Trit Word (`Word21`)**:
  * $3^{21} = 10,460,353,203$ symmetric states in $[-5,230,176,601,\; +5,230,176,601]$.
  * Gate-level combinational ternary logic (`Trit::invert`, `Trit::t_and`, `Trit::t_or`, `Trit::t_mul`, `Trit::consensus`, `Trit::sum_mod3`, `Trit::half_add`, `Trit::full_add`, `Trit::majority_vote`, `Trit::minority_vote`, `Trit::threshold_add`, `Trit::masked_inhibit`, `Trit::cmp_trit`).
  * 21-stage Ripple-Carry Adder (`Word21::gate_ripple_carry_add`) and $21 \times 21 \to 42$-trit Shift-and-Add/Subtract Multiplier (`Word21::gate_shift_and_add_mul42`) where `Trit::Zero` multiplier digits bypass partial-product addition with zero gate switching.
* **Balanced Ternary Fixed-Point (`TernaryFixed` `11.10`)**:
  * Replaces Doom's binary `16.16` `fixed_t` ($2^{16} = 65,536$) with a symmetric **11-trit integer, 10-trit fractional** representation (scale $3^{10} = 59,049$).
  * Physical 10-trit wire slicing (`prod42[10..31]`) performs exact round-to-nearest division by $3^{10}$ without a rounding adder or negative truncation bias (`RMSE = 0.0000` vs binary `16.16` `RMSE = 0.7085`).
* **35-Opcode Orthogonal Ternary ISA**:
  * **Instruction Format (`Word21`)**: `trits[0..13]` (14-trit operand/address, $3^{14} = 4,782,969$ words), `trits[14..15]` (2-trit register/mode selector, $3^2 = 9$ states), `trits[16..20]` (5-trit opcode).
  * **Setun-1958 & Setun-70 Instructions**: `MUL` (`++0` `MULTIPLICATION_ZERO` / `L*T` `B26`), `MAC_POS` (`+++` / `LBT` `B25`), `MAC_NEG` (`++-`), `LOAD_R` (`+-+` / `YRT`), `SMT` (`+-0` / `B19` tritwise multiply), `SAT` (`B21` threshold addition), `SIT` (`B20` $S + T(1 - |S|)$), `BIX` (index loop branch).
  * **REBEL-6 & REBEL-2 Instructions**: `CMP.T` (`<=>` 21-trit magnitude comparator), `MAJV.T` (majority vote), `MINV.T` (minority vote), `STI.T` (21-trit wire inverter), `BCEG` (single-cycle 3-way branch compare equal or greater with two 7-trit relative offsets).
  * **Zero Silent Failures**: Hardware `CpuTrap` exceptions (`InvalidInstruction`, `DivisionByZero`, `MemoryFault`) enforce strict 14-trit memory bounds and trap invalid opcodes or division by zero.
* **Resident Ternary Firmware ROM (`SetunMemory`)**:
  * `ROM_FIXED_MUL_ENTRY` (`PC = 100`), `ROM_FIXED_DIV_ENTRY` (`PC = 120`), `ROM_POINT_ON_SIDE_ENTRY` (`PC = 140`, using Setun-58 `MUL` + `LOAD_R` + `MAC_NEG`), and `ROM_APROX_DIST_ENTRY` (`PC = 220`, using `STI.T` + `CMP.T`) execute Doom's BSP traversal, geometry, and fixed-point math directly through `Cpu::step_checked`.

---

## Emulation Evolution Levels & Branch Roadmap

The project progresses through five architectural levels of balanced ternary emulation, advancing from arithmetic co-processing to full dynamic translation, native compiler targeting, full-system OS emulation, and physical FPGA synthesis:

| Level | Architecture Stage | Scope & Implementation Details | Git Branch | Tag / Status |
| :---: | :--- | :--- | :--- | :---: |
| **Level 0** | **DDA Co-Processor & Gate-Level ALU** | Gate-level combinational arithmetic (`Trit::full_add`, 21-stage ripple-carry adders, 42-trit multipliers, non-restoring ternary division). Symmetric $11.10$ fixed-point arithmetic (`TernaryFixed`). Linear VGA framebuffer in 6-trit trytes (`Tryte6`). C-engine FFI hooks in `r_draw.c` for `R_DrawColumn`, `R_DrawSpan`, `R_DrawFuzzColumn`, and `R_DrawTranslatedColumn`. Strict zero false positives audit. | [`main`](https://github.com/Tryquetra/setun-doom/tree/main) | [`v0.1.0-level-0`](https://github.com/Tryquetra/setun-doom/releases/tag/v0.1.0-level-0) (Completed) |
| **Level 1** | **Dynamic Binary Translation (DBT / JIT)** | Direct execution of standard binary RV32I ELF executables without host C hooks. 32-bit register file ($x0..x31$) mapped directly into `[Word21; 32]`. Basic-block translator fusing branch comparisons into native 3-way `BCEG` instructions. Syscall layer (`ecall`) for console I/O, VGA frame emission, and user input. | [`feat/level-1-dbt-rv32i`](https://github.com/Tryquetra/setun-doom/tree/feat/level-1-dbt-rv32i) | In Progress |
| **Level 2** | **Native Compiler Backend (`LLVM Target`)** | Clang/LLVM target backend (`ternary21-setun`) emitting native 21-trit Setun machine instructions directly from C/C++ source code. Eliminates runtime binary translation; compiles Doom source code directly into native ternary ELF objects with native balanced type representations (`i21`). | `feat/level-2-llvm-target` | Planned |
| **Level 3** | **Full-System Emulation (`Setun-DOS`)** | Autonomous ternary operating environment with native ternary BIOS interrupt vectors. Compatibility layer for DOS `INT 21h` services (DMA file loading for WADs, memory management across $3^{14}$ words). Synthesized FM audio driver emulating OPL2/MIDI using ternary adders. | `feat/level-3-full-system-dos` | Planned |
| **Level 4** | **Physical Hardware Synthesis (FPGA / ASIC)** | Verilog and SystemVerilog implementation utilizing dual-rail encoding (2 bits per trit). Target synthesis for Lattice iCE40 and Xilinx Artix-7 FPGAs. Physical memory controller and hardware VGA DAC rasterization outputting live video to real displays. | `feat/level-4-fpga-synthesis` | Planned |

---

## Benchmark Suite (`Setun-Doom 21-Trit` vs `RV32I 32-Bit`)

| Benchmark / Kernel | Ternary Instrs | Instrs vs `RV32I` | Trit Flips | Digit Flips vs `RV32I` | Dynamic Energy $\Delta V^2$ vs `RV32I` | Host MIPS |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **1. Three-Way Search (`BCEG`)** | `801` | **`-19.66%`** | `887` | **`-21.50%`** | **`-61.06%`** | `7.93` |
| **2. Quicksort 3-Way Partition** | `1,406` | **`-26.19%`** | `2,062` | **`-72.86%`** | **`-87.15%`** | `7.60` |
| **3. BSP `R_PointOnSide` (`ROM`)** | `4,501` | **`-10.05%`** | `18,496` | **`-65.78%`** | **`-88.78%`** | `3.77` |
| **4. Doom `R_DrawColumn` Kernel** | `4,003` | **`-20.04%`** | `10,538` | **`-27.70%`** | **`-64.03%`** | `3.97` |
| **5. 2.4M Stress (Dhrystone Scale)** | `1,500,003` | **`-37.50%`** | `1,950,012` | **`-94.84%`** | **`-96.63%`** | `8.85` |

---

## Building & Running

### 1. Run Unit Tests & Benchmark Suite
```bash
cargo test
cargo run --release
```

### 2. Build & Run Interactive Doom (`doomgeneric_setun` + SDL2)
```bash
cargo build --release
make -C vendor/doomgeneric/doomgeneric -f Makefile.setun
./vendor/doomgeneric/doomgeneric/doomgeneric_setun -iwad ./vendor/DOOM1.WAD
```

---

## License & Third-Party Attributions

* **Setun-Doom Balanced Ternary Emulator (`src/`)**:
  * Copyright (c) 2026 [Robson Cassiano](https://eu.robsoncassiano.software) ([@Tryquetra](https://github.com/Tryquetra)).
  * Licensed under the **MIT License** (see [`LICENSE`](LICENSE)).
* **Doom Engine & `doomgeneric` (`vendor/doomgeneric/`)**:
  * Licensed under the **GNU General Public License v2.0 or later (`GPL-2.0-or-later`)** (see [`vendor/doomgeneric/LICENSE`](vendor/doomgeneric/LICENSE)).
  * Original Doom source code Copyright (C) 1993-1996 **Id Software, Inc.** (John Carmack, John Romero, Dave Taylor, et al.).
  * Chocolate Doom port Copyright (C) 2005-2014 **Simon Howard (`fraggle`)** and **Andrey Budko**.
  * `doomgeneric` port by **Wojtek (`ozkl`)** ([`ozkl/doomgeneric`](https://github.com/ozkl/doomgeneric)).
* **DOOM Shareware Episode 1 Data (`vendor/DOOM1.WAD`)**:
  * Copyright (C) 1993-1996 **Id Software, Inc.** Distributed unmodified at no charge under the DOOM Shareware Distribution License.
* **Academic & Historical Credits**:
  * **Setun (1958) & Setun-70 (1970)**: **Nikolai Petrovich Brusentsov**, **E. A. Zhogolev**, **S. P. Maslov**, and **José Ramil Alvarez** (Computing Center of Moscow State University / MGU); **Stanislav Maslovski** (Setun-70 ISA documentation); archival translations in [`Tryquetra/setun-papers-articles-translations`](https://github.com/Tryquetra/setun-papers-articles-translations) and [`Tryquetra/setun-emulator-original-isa`](https://github.com/Tryquetra/setun-emulator-original-isa).
  * **REBEL-6 & REBEL-2 Extensions**: *"REBEL-6: Instruction Set Extensions on RV32I for Balanced Ternary Computing"* (IEEE ISMVL 2025, [`Soppe/RV32IToREBEL`](https://github.com/Soppe/RV32IToREBEL)) and *"REBEL-2"* (`BCEG` 3-way branch).

See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) for the full compliance record.

