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
