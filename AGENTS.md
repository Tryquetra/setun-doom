# AGENTS.md — Technical Onboarding & Execution Guide for Setun-Doom

This document provides a comprehensive architectural orientation and practical operational guide for autonomous coding agents, LLMs, and researchers working on the **Setun-Doom** repository.

---

## 1. Project Purpose & Essence

**Setun-Doom** is an emulator and hardware co-processor implementing an authentic **21-trit balanced ternary computer architecture** (inspired by the Soviet **Setun-58** and **Setun-70** designed by Nikolai Brusentsov and José Ramil Alvarez, augmented with contemporary **REBEL-6 / REBEL-2** ternary RISC extensions presented at IEEE ISMVL 2025).

The primary mission is executing the original 1993 **Id Software Doom** engine (via `doomgeneric`) on pure balanced ternary arithmetic, eliminating binary two's-complement sign asymmetries and quantifying physical dynamic switching savings (*Digit Flips* and energy metric $\Delta V^2$).

---

## 2. Core Architectural Pillars

### A. 21-Trit Balanced Representation (`Word21`)
* **Radix**: Balanced ternary ($\{-1, 0, +1\}$, represented as `Trit::Negative`, `Trit::Zero`, `Trit::Positive`).
* **State Capacity**: $3^{21} = 10,460,353,203$ states (symmetric integer range $[-5,230,176,601, \, +5,230,176,601]$).
* **Superset of 32-Bit Integers**: Since $3^{21} > 2^{32} = 4,294,967,296$, a single `Word21` represents signed `i32`, unsigned `u32`, and non-overflowing additions of any two 32-bit values without multi-word registers.

### B. Symmetric Fixed-Point 11.10 (`TernaryFixed`)
* Replaces binary 16.16 point fixed representation (`fixed_t`).
* **11 Integer Trits**: Range $[-88,573, \, +88,573]$.
* **10 Fractional Trits**: Fractional scale $3^{10} = 59,049$ ($\approx 1.69 \times 10^{-5}$ resolution).
* **Symmetry**: Unary negation is exact via wire inversion (`prod.gate_invert_trits()`). Sign asymmetry bias is mathematically $0.0000$ (versus $1.0000$ in binary 16.16).

### C. Linear Memory & Tryte6 Framebuffer
* **Memory Capacity**: $3^{14} = 4,782,969$ words linear space (~36.5 MB host allocation).
* **VGA Video Memory**: $320 \times 200 = 64,000$ pixels mapped starting at address `4,600,000` as balanced 6-trit cells (`Tryte6`, $[-364, \, +364]$ holding Doom palette indices `0..255`).

### D. Zero False Positives & Physical Gate Execution
* All additions, subtractions, and memory offsets run through physical 21-stage ripple-carry adders (`Trit::full_add`) recording `adder_carry_propagations`.
* Hardware exceptions (`CpuTrap::DivisionByZero`, `CpuTrap::InvalidInstruction`, `CpuTrap::MemoryFault`) strictly prohibit silent failures.

---

## 3. Emulation Evolution Roadmap & Branches

| Level | Scope & Status | Git Branch | Binary Artifacts |
| :---: | :--- | :--- | :--- |
| **Level 0** | **DDA Co-Processor & Gate-Level ALU**<br>C-engine FFI hooks in `r_draw.c`, `m_fixed.c`, `r_main.c`, and `p_maputl.c` routing fixed-point and BSP partitioning through `libsetun_ffi.a`. Completed. | [`main`](https://github.com/Tryquetra/setun-doom/tree/main) (Tag: `v0.1.0-level-0`) | `libsetun_ffi.a`<br>`doomgeneric_setun` |
| **Level 1** | **Dynamic Binary Translation (DBT / JIT)**<br>Static RISC-V ELF32 parser, 37 official RV32I ISA tests + RV32M validation (100% pass), peephole fusions (`LI.T`, `AIPC.T`, `BCEG`, `STI.T`). Playable bare-metal Doom RV32IM guest (`vendor/doomgeneric/riscv32`, `setun-doom-rv32`) driven by host `ecall` services. Two execution modes: `SetunDbtEngine<true>` (default, gate-accurate, telemetry) and `SetunDbtEngine<false>` (opt-in fast mode, no telemetry, ~50 FPS). Completed. | [`feat/level-1-dbt-rv32i`](https://github.com/Tryquetra/setun-doom/tree/feat/level-1-dbt-rv32i) | `target/release/setun-doom`<br>`target/release/setun-doom-rv32`<br>`vendor/doomgeneric/riscv32/doom_rv32.elf`<br>`vendor/riscv_tests/elf/*.elf`<br>`vendor/riscv_tests/rv32m/*.elf` |
| **Level 1.5** | **Pure REBEL-6 Core (`feat/rebel6-pure`)**<br>32-trit REBEL-6 core with 24-trit words, 729 registers, Harvard memories and a `.tas` assembler. Doom is compiled with clang for RV32I, translated by the R2R reference tool, assembled and executed by the core. 37/37 official RV32I tests, Dhrystone final state identical to R2R, E1M1 frame rendered. Experimental. | [`feat/rebel6-pure`](https://github.com/Tryquetra/setun-doom/tree/feat/rebel6-pure) | `target/release/rebel6_doom`<br>`target/release/rebel6_run`<br>`target/release/rebel6_asm`<br>`vendor/doomgeneric/riscv32/build/rebel6/doom_all.tas` |
| **Level 2** | **Native LLVM Backend (`ternary21-setun`)**<br>Emits native 21-trit machine instructions from C/C++ without DBT layer. Planned. | `feat/level-2-llvm-target` | Clang/LLVM target |
| **Level 3** | **Full-System DOS Emulation (`Setun-DOS`)**<br>Ternary operating environment with BIOS interrupts, TFX flat loader, and DMA WAD loading inspired by KolibriOS architecture. Completed. | [`feat/level-3-full-system-dos`](https://github.com/Tryquetra/setun-doom/tree/feat/level-3-full-system-dos) | `target/release/setun_dos`<br>`src/dos/` |
| **Level 4** | **Physical FPGA Synthesis**<br>Lattice iCE40 and Xilinx Artix-7 dual-rail Verilog synthesis with physical VGA DAC. Planned. | `feat/level-4-fpga-synthesis` | Bitstreams |

---

## 4. How to Build & Run

### A. Prerequisites
* **Rust**: Toolchain 1.80+ with `cargo`.
* **C Toolchain**: `gcc`, `make`, `pkg-config`, `SDL2` (`libsdl2-dev` / `sdl2`).
* **Cross-Compilers (for Level 1 RV32IM)**: `clang` with RISC-V support (`--target=riscv32-unknown-elf`) and `ld.lld`.

### B. Building the Release Suite
```bash
# Build testbench binary (rlib only, keeps the no_std RISC-V unikernel buildable)
cargo build --release

# Build the C ABI artifacts consumed by the doomgeneric engine
cargo build --release -p setun-ffi

# Cross-compile the Level 1 RV32IM Doom guest (clang + rust-lld)
make -C vendor/doomgeneric/riscv32

# Run unit tests and integration tests (74 tests, ~2.8s execution)
cargo test
```

### C. Running the Benchmarks & Telemetry Testbench
```bash
./target/release/setun-doom
```
Outputs:
* Complete 21-trit architectural configuration.
* BSP 3D traversal and VGA render throughput (~190+ FPS).
* IEEE ISMVL 2025 comparative table (Setun 21-trit vs RV32I 32-bit).
* Level 1 DBT official 37 RISC-V ISA test pass confirmation.
* Bare-metal Doom RV32IM execution output.

### D. Running Interactive Doom with Real-Time Ternary HUD
```bash
# Launch native Doom engine with SDL2 graphics window (640x400)
./vendor/doomgeneric/doomgeneric/doomgeneric_setun -iwad vendor/DOOM1.WAD
```

#### Controls & HUD Features:
* **Movement & Action**: `WASD` / Arrow keys to move, `Ctrl` to shoot, `Space` to open doors, `Tab` for Automap.
* **Ternary Telemetry HUD (Top Semi-Transparent Bar)**:
  * `SETUN ACCUMULATOR (21 TRITS)`: 21 neon lamps showing live accumulator trits (Green $= +1$, Gray $= 0$, Amber $= -1$).
  * `ENERGY SAVE`: Real-time percentage of physical switching reduction ($\Delta V^2$).
  * `BSP SIDE`: Trichotomous plane partition status (`FRONT`, `BACK`, `PLANE`).
  * `ALU CARRY GATES`: Yellow pulse train indicating adder carry activity.
  * **Toggle HUD Key**: Press **`F12`** to toggle the HUD overlay on/off.

### E. REBEL-6 Doom pipeline

The `feat/rebel6-pure` branch executes `doomgeneric` on a 32-trit REBEL-6 core with 24-trit
words, 729 registers and Harvard memories. The 21-trit ISA and the DBT are not used on this path.

```bash
./scripts/setup_r2r.sh                 # clone the R2R fork branch and build the translator
python3 scripts/build_rebel6_doom.py   # C -> RV32I assembly -> R2R -> doom_all.tas
cargo build --release --bin rebel6_doom --bin rebel6_run --bin rebel6_asm
./target/release/rebel6_doom vendor/doomgeneric/riscv32/build/rebel6/doom_all.tas 2
```

Current results on the branch:

| Check | Result |
| :-- | :-- |
| `cargo test` | 109/109 |
| Official RV32I tests | 37/37 |
| Doom image | 75,509 instructions, 4,357,942 data trytes, 243,848 BSS trytes |
| Doom frame 2 | 63,847 non-zero pixels, E1M1 with HUD |
| Frame 2 | 31,188,234 cycles, ~54 s, ~0.15 FPS steady |
| Frame parity with the 21-trit reference | 97.5% |

Carry telemetry classifies every ripple-adder carry as `+1`, `-1` or `0` and measures absorption
and chain length. Measured absorption is 64.8% on Doom, 70.7% on BSP and 73.7% on the RV32I test
suite. See [`docs/carry-telemetry.md`](file:///home/robson/coding/setun-doom/docs/carry-telemetry.md).

AVX2 kernels cover bitsliced tritwise operations and the framebuffer palette conversion. The
palette path is integrated; a vectorized `R_DrawSpan` measured 0.94x and was removed. See
[`docs/simd.md`](file:///home/robson/coding/setun-doom/docs/simd.md).

Known limits: the pipeline concatenates translation units instead of linking objects, with
unit-local symbols renamed and duplicate labels rejected; there are no relocation records, so
unused code is not dropped; `.bss` is NOBITS and follows the initialized data; `.p2align` is
opt-in because honouring clang's alignment by default shifts the image and breaks the frame; the
data space is a flat byte-addressed window; the guest uses `-march=rv32i` with soft multiply and
divide; the player weapon sprite is not rendered.

---

## 5. Key Source Code Map

* [`src/trit.rs`](file:///home/robson/coding/setun-doom/src/trit.rs): Trit definition, full adders, truth tables, and dual-bit packing.
* [`src/word.rs`](file:///home/robson/coding/setun-doom/src/word.rs): `Word21` 21-trit word, ripple-carry addition, Alvarez conversion algorithm ($2 \leftrightarrow 3$).
* [`src/fixed.rs`](file:///home/robson/coding/setun-doom/src/fixed.rs): `TernaryFixed` symmetric 11.10 fixed-point arithmetic and Doom 16.16 bridge.
* [`src/memory.rs`](file:///home/robson/coding/setun-doom/src/memory.rs): Linear $3^{14}$-word memory, VGA framebuffer translation, and MMIO registers.
* [`src/cpu.rs`](file:///home/robson/coding/setun-doom/src/cpu.rs): 35-opcode Setun/REBEL-6 CPU pipeline, hardware traps, and switching telemetry.
* [`src/elf.rs`](file:///home/robson/coding/setun-doom/src/elf.rs): Static RISC-V ELF32 parser and memory segment loader.
* [`src/dbt.rs`](file:///home/robson/coding/setun-doom/src/dbt.rs): Dynamic binary translator (RV32IM to Setun 21-trit), flat operation arena with a direct PC index, peephole fusions, and the `VERACITY` const-generic execution mode.
* [`src/bsp.rs`](file:///home/robson/coding/setun-doom/src/bsp.rs) / [`src/render.rs`](file:///home/robson/coding/setun-doom/src/render.rs): Trichotomous BSP geometry clipper (`solidsegs`) and 3D column rasterizer.
* [`src/mmo.rs`](file:///home/robson/coding/setun-doom/src/mmo.rs): FFI bridge and C ABI exports linking `doomgeneric` to `libsetun_ffi.a` (`crates/setun-ffi`).
* [`src/rebel6/`](file:///home/robson/coding/setun-doom/src/rebel6/): 32-trit REBEL-6 core (`Word24`, instruction encoding, 729 registers, Harvard memories, `.tas` assembler) and the R2R Doom pipeline. See [`docs/rebel6-pipeline.md`](file:///home/robson/coding/setun-doom/docs/rebel6-pipeline.md).
* [`src/simd.rs`](file:///home/robson/coding/setun-doom/src/simd.rs): AVX2 bitsliced ternary kernels and the framebuffer palette conversion, with scalar fallback and runtime feature detection.
* [`src/bin/rebel6_run.rs`](file:///home/robson/coding/setun-doom/src/bin/rebel6_run.rs) / [`src/bin/rebel6_asm.rs`](file:///home/robson/coding/setun-doom/src/bin/rebel6_asm.rs) / [`src/bin/rebel6_doom.rs`](file:///home/robson/coding/setun-doom/src/bin/rebel6_doom.rs): `.tas` runner, assembler CLI and Doom `ecall` host.
* [`scripts/setup_r2r.sh`](file:///home/robson/coding/setun-doom/scripts/setup_r2r.sh) / [`scripts/build_rebel6_doom.py`](file:///home/robson/coding/setun-doom/scripts/build_rebel6_doom.py): R2R clone and build, plus the C-to-`.tas` pipeline. The compatibility changes live on the `setun-doom-compat` branch of [`Tryquetra/RV32IToREBEL`](https://github.com/Tryquetra/RV32IToREBEL), forked from `Soppe/RV32IToREBEL`.
* [`docs/carry-telemetry.md`](file:///home/robson/coding/setun-doom/docs/carry-telemetry.md) / [`docs/simd.md`](file:///home/robson/coding/setun-doom/docs/simd.md): signed carry measurements and SIMD results.
* [`src/stack_machine.rs`](file:///home/robson/coding/setun-doom/src/stack_machine.rs): Setun-70 two-stack RPN machine over ternary memory (operand and return stacks, tryte-stream decoder, textual assembler with labels and typed errors), independent of the accumulator CPU and the DBT.
* [`src/dos/`](file:///home/robson/coding/setun-doom/src/dos/): Setun-DOS kernel, static memory mapping, TFX loader, trichotomic syscalls, and Doom runner.
* [`vendor/doomgeneric/doomgeneric/doomgeneric_setun.c`](file:///home/robson/coding/setun-doom/vendor/doomgeneric/doomgeneric/doomgeneric_setun.c): SDL2 platform driver with interactive telemetry HUD.
* [`vendor/doomgeneric/riscv32/`](file:///home/robson/coding/setun-doom/vendor/doomgeneric/riscv32): freestanding RV32IM guest port (libc shim with word-wise copies, soft-float builtins, `ecall` ABI, linker script, Makefile).
* [`src/bin/setun_doom_rv32.rs`](file:///home/robson/coding/setun-doom/src/bin/setun_doom_rv32.rs): Level 1 host driver and `ecall` platform services (framebuffer, palette, keyboard, clock, WAD reads).
* [`tests/integration_dbt.rs`](file:///home/robson/coding/setun-doom/tests/integration_dbt.rs): External black-box integration tests.

---

## 6. Strict Development & Contribution Rules

1. **No Silent Failures**:
   Never discard results using `let _ =` on fallible operations without logging or handling. Propagate typed errors or trigger explicit traps.
2. **Preserve Gate-Level Veracity**:
   Never replace balanced ternary gate calculations with native binary host math shortcuts. All arithmetic additions must pass through `gate_ripple_carry_add`.
3. **Documentation & Formatting Standards**:
   Maintain docstrings, preserve file links in markdown using `file:///` format, and adhere strictly to repository language guidelines.
4. **SIMD Is Measured, Not Assumed**:
   AVX2 code lives behind `cfg(all(feature = "std", target_arch = "x86_64"))` with a scalar fallback. A kernel enters a hot path only after an A/B benchmark. The vectorized `R_DrawSpan` measured 0.94x and was removed; the palette conversion measured 1.5x to 2.0x and stayed.
5. **REBEL-6 Core Stays `no_std`**:
   `src/rebel6/{word,encoding,mem,cpu}.rs` compile without `std`. The assembler and binaries require `std`. `cargo build --no-default-features --lib` must pass.
6. **R2R Is External and Forked**:
   `vendor/RV32IToREBEL` is gitignored. `scripts/setup_r2r.sh` clones the fork `Tryquetra/RV32IToREBEL` (from `Soppe/RV32IToREBEL`) and checks out the `setun-doom-compat` branch, which carries the compatibility changes plus the tooling added on top, then builds it. A fresh worktree has no translator until the script runs. Override with `R2R_REPO` and `R2R_REF`.

   The branch now also provides: multi-input linking with per-unit symbol scoping (`--link`), relocatable `.mto` objects (`--emit-obj`), R6IM machine images (`--emit-image`, `--no-tas`), the official RV32I suite (`--run-tests`), a differential simulator (`--diff`), JSON telemetry (`--stats`, `--simulate`), typed diagnostics with source positions, range-checked immediates, `.comm` and the other assembler directives, RV32M lowering to the soft helpers, and zero compiler warnings.
