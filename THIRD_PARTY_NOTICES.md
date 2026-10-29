# Third-Party Licenses, Notices, and Academic Attributions

This document records the copyright notices, license terms, and academic attributions for all third-party software, assets, and research papers integrated or referenced in **Setun-Doom**.

---

## 1. Setun-Doom Balanced Ternary Emulator (`src/`)

* **Author**: [Robson Cassiano](https://eu.robsoncassiano.software)
* **Organization**: [@Tryquetra](https://github.com/Tryquetra)
* **License**: MIT License (see [`LICENSE`](LICENSE))

---

## 2. Doom Engine & `doomgeneric` (`vendor/doomgeneric/`)

* **Upstream Project**: [`ozkl/doomgeneric`](https://github.com/ozkl/doomgeneric) by Wojtek (`ozkl`)
* **Derived From**:
  * **Linux Doom (1997)**: Copyright (C) 1993-1996 Id Software, Inc. (John Carmack, John Romero, Dave Taylor, et al.)
  * **Chocolate Doom**: Copyright (C) 2005-2014 Simon Howard (`fraggle`), Copyright (C) 2005-2006 Andrey Budko
* **License**: GNU General Public License v2.0 or later (`GPL-2.0-or-later`). Full license text is located in [`vendor/doomgeneric/LICENSE`](vendor/doomgeneric/LICENSE).
* **GPLv2 Section 2(a) Record of Modifications**:
  The following files in `vendor/doomgeneric/doomgeneric/` were modified or added on **2026-10-25** by **[Robson Cassiano](https://eu.robsoncassiano.software)** to route fixed-point arithmetic, BSP geometric determinants, distance estimation, column/span texture rasterization, and VGA palette/framebuffer translation through the 21-trit Setun-Doom balanced ternary CPU (`libsetun_doom.a`):
  1. `vendor/doomgeneric/doomgeneric/m_fixed.c` (hooked `FixedMul` and `FixedDiv` to `setun_FixedMul` / `setun_FixedDiv` under `SETUN_TERNARY_ALU`)
  2. `vendor/doomgeneric/doomgeneric/r_main.c` (hooked `R_PointOnSide` and `R_PointOnSegSide` to `setun_PointOnSide` under `SETUN_TERNARY_ALU`)
  3. `vendor/doomgeneric/doomgeneric/p_maputl.c` (hooked `P_AproxDistance` and `P_PointOnLineSide` to `setun_AproxDistance` / `setun_PointOnSide` under `SETUN_TERNARY_ALU`)
  4. `vendor/doomgeneric/doomgeneric/r_draw.c` (hooked `R_DrawColumn` and `R_DrawSpan` to `setun_DrawColumn` / `setun_DrawSpan` under `SETUN_TERNARY_ALU`)
  5. `vendor/doomgeneric/doomgeneric/i_video.c` (hooked `I_SetPalette` to `setun_SyncPalette` under `SETUN_TERNARY_ALU`)
  6. `vendor/doomgeneric/doomgeneric/doomgeneric_setun.c` and `vendor/doomgeneric/doomgeneric/Makefile.setun` (added SDL2/headless Setun-Doom platform driver and build configuration)
  7. `vendor/doomgeneric/doomgeneric/setun_hooks.h`, plus counter increments in `r_draw.c`, `m_fixed.c`, `r_main.c` and `p_maputl.c` (added on **2026-10-27** to cross-check that the C engine actually invokes the ternary hooks, comparing C-side call/pixel counts against the Rust bridge)
  8. `vendor/doomgeneric/doomgeneric/m_fixed.c` (modified on **2026-10-27**: `FixedMul` now computes a 64-bit product and `FixedDiv` uses a 32-bit long-division fraction, because the original 32-bit split algorithms overflow for large operands and corrupted the projection focal length in the RV32IM guest). The freestanding guest port under `vendor/doomgeneric/riscv32/` (headers, libc shim, soft-float builtins, `ecall` platform layer, linker script, Makefile) is original code released under the same GPL-2.0-or-later terms as the engine it links.

---

## 3. DOOM Shareware Data (`vendor/DOOM1.WAD`)

* **Copyright**: Copyright (C) 1993-1996 Id Software, Inc. All rights reserved.
* **License**: DOOM Shareware Distribution License.
* **Notice**: `vendor/DOOM1.WAD` is the unmodified Shareware Episode 1 (*Knee-Deep in the Dead*, v1.9) IWAD data file created by Id Software, Inc. Unlike the C source code in `vendor/doomgeneric/`, `DOOM1.WAD` is not covered by the GNU General Public License; it is redistributed unmodified and free of charge in accordance with Id Software's Shareware redistribution terms.

---

## 4. Official RISC-V RV32I ISA Test Suite (`vendor/riscv_tests/`)

* **Upstream Sources**:
  * [`riscv-software-src/riscv-tests`](https://github.com/riscv-software-src/riscv-tests) — Copyright (c) 2012-2015, The Regents of the University of California (Regents). All Rights Reserved.
  * Preprocessed RV32I assembly adaptation from [`Soppe/RV32IToREBEL`](https://github.com/Soppe/RV32IToREBEL) (`RiscvTests/PreprocessedAssembly/`).
* **License**: BSD 3-Clause License (`BSD-3-Clause`). Full license text is located in [`vendor/riscv_tests/LICENSE`](vendor/riscv_tests/LICENSE).
* **Contents**: 37 standalone RV32I instruction set compliance tests (`add`, `addi`, `and`, `andi`, `auipc`, `beq`, `bge`, `bgeu`, `blt`, `bltu`, `bne`, `jal`, `jalr`, `lb`, `lbu`, `lh`, `lhu`, `lui`, `lw`, `or`, `ori`, `sb`, `sh`, `simple`, `sll`, `slli`, `slt`, `slti`, `sltiu`, `sltu`, `sra`, `srai`, `srl`, `srli`, `sub`, `sw`, `xor`, `xori`) in both preprocessed assembly (`vendor/riscv_tests/asm/*.s`) and statically linked RISC-V `ELF32` binaries (`vendor/riscv_tests/elf/*.elf`).

---

## 5. Academic & Historical Architecture Attributions

### A. Setun (1958) & Setun-70 (1970) Balanced Ternary Computers
* **Architects & Researchers**: **Nikolai Petrovich Brusentsov**, **E. A. Zhogolev**, **S. P. Maslov**, and **José Ramil Alvarez** (Problem Laboratory of Ternary Computers, Computing Center of Moscow State University / MGU).
* **Primary Sources & Translations (`@Tryquetra`)**:
  * N. P. Brusentsov, E. A. Zhogolev, S. P. Maslov, *"Общая характеристика малой цифровой машины «Сетунь-70»"* (*General Characteristics of the Small Digital Machine Setun-70*), Computing Center of MSU, 1971.
  * Stanislav Maslovski, *"Описание набора инструкций Сетунь-70"* (*Description of the Setun-70 Instruction Set*), 2023 (`B1..B27` and `S1..S27` syllabic operations, `SMT`, `SAT`, `SIT`, `L*T`, `LBT`, `LHT`).
  * J. Ramil Alvarez, *"Простые алгоритмы перевода $p \to p-1$ и $p \to p+1$"* (*Simple Conversion Algorithms $p \to p-1$ and $p \to p+1$*), Computing Center of MSU, 1974 (recurrent hardware base conversion $2 \leftrightarrow 3$).
  * Archival repositories maintained by [@Tryquetra](https://github.com/Tryquetra):
    * [`Tryquetra/setun-papers-articles-translations`](https://github.com/Tryquetra/setun-papers-articles-translations)
    * [`Tryquetra/setun-emulator-original-isa`](https://github.com/Tryquetra/setun-emulator-original-isa)
    * [`Tryquetra/setun-online-simulator`](https://github.com/Tryquetra/setun-online-simulator)
    * [`Tryquetra/ternary-computing-terminology`](https://github.com/Tryquetra/ternary-computing-terminology)

### B. REBEL-6 & REBEL-2 Balanced Ternary RISC Extensions
* **Paper**:
  * Steven Bos, Vetle Bodahl, Ole Christian Moholth and Henning Gundersen, *"REBEL-6: A 32-trit balanced ternary instruction set architecture with R2R compiler pipeline for C"*, 2025 IEEE 55th International Symposium on Multiple-Valued Logic (ISMVL), pp. 98-103, DOI [10.1109/ISMVL64713.2025.00028](https://doi.org/10.1109/ISMVL64713.2025.00028).
  * The accepted manuscript is distributed by the Norwegian Research Information Repository (NVA/Sikt) under the **Creative Commons Attribution 4.0 International (CC BY 4.0)** license and is included unmodified at [`docs/papers/rebel-6-ismvl-2025.pdf`](docs/papers/rebel-6-ismvl-2025.pdf), with attribution to the authors above. The version of record is available at IEEE.
  * Instructions `CMP.T`, `MAJV.T`, `MINV.T`, `STI.T` and the Digit Flips telemetry methodology come from this paper, adapted here to a 21-trit word as documented in [`docs/rebel6.md`](docs/rebel6.md).
  * Steven Bos, *"Beyond 0 and 1: A mixed radix design and verification workflow for modern ternary computers"*, PhD dissertation, University of South-Eastern Norway, 2024, ISBN 978-82-7206-854-6 (print) / 978-82-7206-855-3 (online). Included unmodified at [`docs/papers/bos-2024-mixed-radix-thesis.pdf`](docs/papers/bos-2024-mixed-radix-thesis.pdf) under the **Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International (CC BY-NC-SA 4.0)** license, with attribution to the author. The mixed-radix workflow, the MRCS EDA tool and the R2R pipeline background cited as reference [9] of the REBEL-6 paper are described there.
* **Reference Implementation**:
  * [`Soppe/RV32IToREBEL`](https://github.com/Soppe/RV32IToREBEL) by Vetle Bodahl, MIT license, vendored at `vendor/RV32IToREBEL`. The REBEL-2 predecessor ISA defines the `BCEG` single-cycle 3-way branch compare equal or greater.
