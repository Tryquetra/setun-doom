# Signed carry telemetry

The 21-trit ripple-carry adder (`Word21::gate_ripple_carry_add`) already counts every non-zero
carry. The `feat/rebel6-pure` branch adds `CarryStats`, which classifies each carry by sign and
measures chain length and absorption.

## Metrics

| Field | Meaning |
| :-- | :-- |
| `positive` | stages whose carry-out is `+1` |
| `negative` | stages whose carry-out is `-1` |
| `zero` | stages whose carry-out is `0` |
| `absorbed` | stages where a non-zero carry-in produced `carry_out = 0` |
| `chains` | number of maximal non-zero carry runs |
| `chain_sum` | total stages covered by those runs |
| `chain_max` | longest non-zero carry run |

`CarryStats::absorption_pct` is `absorbed / (positive + negative)`. `average_chain` is
`chain_sum / chains`.

## Where it is collected

| Path | Function |
| :-- | :-- |
| 21-trit CPU | `Cpu::add_with_carry_stats`, used by `Add`, `Sub`, `AddImm`, `AddIxImm`, `AddIx`, `DecIxJmpGe`, `MacPos`, `MacNeg` |
| DBT | `SetunDbtEngine::gate_add_i32` and `gate_to_unsigned32_word21` |
| Report | `PowerTelemetry::carry_stats`, printed by `setun-doom` and `setun-doom-rv32` |

Multiply-internal carries from `gate_shift_and_add_mul42` remain counted in
`adder_carry_propagations` but are not classified by sign.

## Measured results

`cargo run --release --bin setun-doom -- --no-gui`:

| Workload | `+1` | `-1` | `0` | Absorbed | Absorption | Chains | Average | Max |
| :-- | --: | --: | --: | --: | --: | --: | --: | --: |
| BSP `R_PointOnSide` | 203 | 578 | 11,819 | 552 | 70.7% | 552 | 1.41 | 5 |
| DBT suite, 37 RV32I tests | 1,284 | 839 | 127,993 | 1,565 | 73.7% | 1,565 | 1.36 | 6 |
| Doom RV32IM | 469,350 | 650,231 | 13,196,497 | 725,425 | 64.8% | 725,425 | 1.54 | 11 |

Derived values:

| Workload | Non-zero carries | Negative share | Zero share of stages | Additions sampled |
| :-- | --: | --: | --: | --: |
| BSP `R_PointOnSide` | 781 | 74.0% | 93.8% | 600 |
| DBT suite | 2,123 | 39.5% | 98.4% | 6,196 |
| Doom RV32IM | 1,119,581 | 58.1% | 92.2% | 681,718 |

## Interpretation

- Absorption is high: 64.8% to 73.7% of non-zero carries terminate on the next stage. In binary,
  a carry entering a `1` always propagates.
- Carry chains are short: 1.36 to 1.54 stages on average, with maxima between 5 and 11.
- Negative carries dominate on BSP and Doom. Subtraction is implemented as adding the inverted
  operand, and the inversion produces `-1` carries. This is the signed carry doing real work.
- Most stages have no carry: 92.2% to 98.4%. For uniform random operands the model predicts 75%.
  Real workloads are even quieter.

## Caveats

- These workloads are binary-compatible RV32I code executed on 21-trit words. The ternary-native
  BSP renderer uses `TernaryFixed`, not this telemetry path.
- The Doom run still has 7.64% more trit flips than the binary reference. Shorter carry chains do
  not automatically win when the workload pays conversion and emulation costs.
- The signed carry benefit is data-dependent. The metric exists to measure it, not to assume it.
