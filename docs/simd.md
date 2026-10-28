# SIMD experiments

The branch contains AVX2 kernels in `src/simd.rs`, a benchmark in `src/bin/simd_bench.rs`, and a
vectorized framebuffer palette conversion used by `Memory::render_framebuffer_rgba`.

Run:

```bash
cargo run --release --bin simd_bench
```

## Tritwise kernels

Bitsliced `P/N` planes, one 256-bit AVX2 pair per 256 trits.

| Working set | `AND.T` | `OR.T` | `XOR.T` | Negation |
| :-- | --: | --: | --: | --: |
| 1 Mi trits, memory-bound | 2.9x | 1.7x | 4.3x | 2.0x |
| 32 Ki trits, L1-resident | 4.8x | 4.8x | 4.7x | 2.8x |

The scalar baseline is compiler-optimized, so these gains are conservative.

## Framebuffer palette conversion

`Memory::render_framebuffer_rgba` now calls `simd::palette_rgba`, which gathers four RGBA pixels per
iteration with `_mm256_i64gather_epi32`.

| Version | Time for 64,000 pixels | Throughput |
| :-- | --: | --: |
| Scalar | 57.0 us | 1.12 Gpixel/s |
| AVX2 | 27.9 us | 2.29 Gpixel/s |
| Gain | 2.04x | |

Out-of-range cells fall back to the scalar `Tryte6::to_palette_index` rule.

## Rejected: vectorized `R_DrawSpan`

A four-pixel AVX2 span kernel was implemented and measured. It converts fixed-point `u/v` with
`f64` division, gathers texels with `_mm_i32gather_epi32`, and stores four `i64` cells per
iteration.

| Version | Time for 320 pixels | Throughput |
| :-- | --: | --: |
| Scalar | 0.381 us | 840 Mpixel/s |
| AVX2 | 0.405 us | 790 Mpixel/s |
| Gain | 0.94x | |

The vector path was removed. Division by the constant `59_049` is strength-reduced to a
multiply and shift by the compiler, and the gather plus `i64` conversion costs more than the
scalar loop for spans of 320 pixels.

## Next candidates

- Longer spans or several spans batched per call, where gather setup amortizes.
- AVX-512 `vpscatterdd` for strided column writes, which AVX2 cannot do.
- Palette conversion for other formats, where the gain is already proven.
- Tritwise kernels applied to ternary neural-network or packed-logic workloads, not the rasterizer.
