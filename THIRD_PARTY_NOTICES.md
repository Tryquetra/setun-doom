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

---

## 3. DOOM Shareware Data (`vendor/DOOM1.WAD`)

* **Copyright**: Copyright (C) 1993-1996 Id Software, Inc. All rights reserved.
* **License**: DOOM Shareware Distribution License.
* **Notice**: `vendor/DOOM1.WAD` is the unmodified Shareware Episode 1 (*Knee-Deep in the Dead*, v1.9) IWAD data file created by Id Software, Inc. Unlike the C source code in `vendor/doomgeneric/`, `DOOM1.WAD` is not covered by the GNU General Public License; it is redistributed unmodified and free of charge in accordance with Id Software's Shareware redistribution terms.

---

## 4. Academic & Historical Architecture Attributions

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
* **Papers & Reference Implementation**:
  * *"REBEL-6: Instruction Set Extensions on RV32I for Balanced Ternary Computing"*, IEEE International Symposium on Multiple-Valued Logic (ISMVL), 2025, and reference repository [`Soppe/RV32IToREBEL`](https://github.com/Soppe/RV32IToREBEL) (instructions `CMP.T`, `MAJV.T`, `MINV.T`, `STI.T`, and the Digit Flips dynamic switching telemetry methodology).
  * *"REBEL-2"* balanced ternary branch extension (`BCEG` single-cycle 3-way branch compare equal or greater).
