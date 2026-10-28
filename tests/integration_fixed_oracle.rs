// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Oracle comparison for the ternary fixed-point bridge.
//!
//! Compares `setun_FixedMul` and `setun_FixedDiv` against:
//! 1. The exact real value `a * b / 2^16`, which is what Doom's `fixed_t` represents.
//! 2. The original C algorithms from `vendor/doomgeneric/doomgeneric/m_fixed.c` (the `#else`
//!    branch), transcribed with C's 32-bit wrapping semantics.
//!
//! The bridge quantizes operands to `3^-10` (11.10 trits) and results back to `2^-16`, so exact
//! equality with the C path is not expected. Each assertion bounds the measured error by the
//! error propagation that this quantization implies, which catches wrong shifts, signs or
//! truncation directions.

use setun_doom::mmo::{setun_FixedDiv, setun_FixedMul};

/// Doom `FRACUNIT` for 16.16 fixed point.
const FRACUNIT: f64 = 65_536.0;

/// Half a ternary fixed-point step in real units: `0.5 / 3^10`.
const HALF_TERNARY_STEP: f64 = 0.5 / 59_049.0;

/// Largest magnitude the ternary 11.10 integer part can hold, in 16.16 units.
const TERNARY_MAX_16_16: f64 = 88_573.0;

/// Faithful transcription of the vendored C `FixedMul` (split 32-bit algorithm).
fn c_fixed_mul(a: i32, b: i32) -> i32 {
    let ahi = a >> 16;
    let alo = a & 0xFFFF;
    ahi.wrapping_mul(b).wrapping_add(alo.wrapping_mul(b) >> 16)
}

/// Faithful transcription of the vendored C `FixedDiv` (quotient/remainder algorithm).
fn c_fixed_div(a: i32, b: i32) -> i32 {
    if (a.unsigned_abs() >> 14) >= b.unsigned_abs() {
        return if (a ^ b) < 0 { i32::MIN } else { i32::MAX };
    }
    let q = a / b;
    let r = a % b;
    q.wrapping_shl(16).wrapping_add((r << 16) / b)
}

/// Returns true when the vendored split `FixedMul` stays inside `i32` at every step.
///
/// Doom's 32-bit split algorithm overflows in C when `alo * b` or the final sum leaves `i32`;
/// those inputs are undefined behavior in the C engine itself and are excluded from comparison.
fn c_fixed_mul_is_defined(a: i32, b: i32) -> bool {
    let ahi = (a >> 16) as i64;
    let alo = (a & 0xFFFF) as i64;
    let high = ahi * (b as i64);
    let low = alo * (b as i64);
    let shifted = low >> 16;
    let fits = |v: i64| (i32::MIN as i64..=i32::MAX as i64).contains(&v);
    fits(high) && fits(low) && fits(high + shifted)
}

/// Returns true when the vendored `FixedDiv` stays inside `i32` at every step.
fn c_fixed_div_is_defined(a: i32, b: i32) -> bool {
    if b == 0 || (a.unsigned_abs() >> 14) >= b.unsigned_abs() {
        return true;
    }
    let q = (a / b) as i64;
    let r = (a % b) as i64;
    let fits = |v: i64| (i32::MIN as i64..=i32::MAX as i64).contains(&v);
    fits(q << 16) && fits(r << 16) && fits((q << 16) + ((r << 16) / b as i64))
}

/// Deterministic xorshift64* generator, so the corpus is reproducible across runs.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_i32(&mut self) -> i32 {
        self.next_u64() as i32
    }

    fn next_bounded(&mut self, limit: i64) -> i32 {
        let span = 2 * limit + 1;
        ((self.next_u64() % span as u64) as i64 - limit) as i32
    }

    fn next_in(&mut self, lo: i64, hi: i64) -> i32 {
        let span = hi - lo + 1;
        (lo + (self.next_u64() % span as u64) as i64) as i32
    }
}

/// Edge values that stress sign, saturation and small fractions.
const EDGES: [i32; 16] = [
    0,
    1,
    -1,
    2,
    -2,
    32_768,
    -32_768,
    65_535,
    -65_535,
    65_536,
    -65_536,
    1 << 20,
    -(1 << 20),
    46_341,
    -46_341,
    i32::MAX,
];

#[test]
fn test_fixed_mul_against_exact_oracle() {
    let mut rng = Rng(0x5E70_D00D_0000_0001);
    let mut samples = 0u64;
    let mut skipped = 0u64;
    let mut max_abs_error = 0.0f64;
    let mut max_ratio = 0.0f64;
    let mut max_c_divergence = 0.0f64;
    let mut exact_c_matches = 0u64;
    let mut worst: Vec<(f64, i32, i32, i32, f64, f64)> = Vec::new();

    let mut check = |a: i32, b: i32| {
        let ternary = setun_FixedMul(a, b) as f64;
        let real = (a as f64) * (b as f64) / FRACUNIT;

        // Skip pairs the representation cannot hold or where the C split algorithm itself
        // wraps: those compare representations, not arithmetic.
        if real.abs() > TERNARY_MAX_16_16 || !c_fixed_mul_is_defined(a, b) {
            skipped += 1;
            return;
        }

        let abs_error = (ternary - real).abs();
        let bound = (a.abs() as f64 + b.abs() as f64) * HALF_TERNARY_STEP
            + 2.0 * HALF_TERNARY_STEP * FRACUNIT;
        max_abs_error = max_abs_error.max(abs_error);
        let ratio = abs_error / bound.max(f64::EPSILON);
        max_ratio = max_ratio.max(ratio);

        // Divergence from the actual C algorithm, which itself truncates toward zero.
        let c_value = c_fixed_mul(a, b) as f64;
        let c_divergence = (ternary - c_value).abs();
        max_c_divergence = max_c_divergence.max(c_divergence);
        if ternary as i32 == c_fixed_mul(a, b) {
            exact_c_matches += 1;
        }
        assert!(
            c_divergence <= bound + 1.0,
            "ternary FixedMul diverges from C by {c_divergence:.3} units (a={a}, b={b})"
        );
        if worst.len() < 64 || abs_error > worst.last().map(|w| w.4).unwrap_or(0.0) {
            worst.push((ratio, a, b, ternary as i32, real, bound));
            worst.sort_by(|x, y| y.4.total_cmp(&x.4));
            worst.truncate(8);
        }
        samples += 1;
    };

    for &a in EDGES.iter() {
        for &b in EDGES.iter() {
            check(a, b);
        }
    }
    // Operand magnitudes that keep the product inside the 11.10 range.
    for _ in 0..6_000 {
        check(rng.next_bounded(1 << 17), rng.next_bounded(1 << 15));
    }
    for _ in 0..6_000 {
        check(rng.next_bounded(1 << 16), rng.next_bounded(1 << 16));
    }

    for (ratio, a, b, ternary, real, bound) in worst.iter().take(4) {
        println!("  mul worst: a={a} b={b} ternary={ternary} real={real:.3} bound={bound:.3} ratio={ratio:.3}");
    }
    println!(
        "FixedMul: {samples} compared, {skipped} skipped (overflow/range), \
         max |error| = {max_abs_error:.3} units of 2^-16, max error/bound = {max_ratio:.3}, \
         max divergence from C = {max_c_divergence:.3} units, exact C matches = \
         {exact_c_matches}/{samples}"
    );

    assert!(samples > 7_000, "corpus must exercise the normal range");
    assert!(
        max_ratio <= 1.001,
        "ternary FixedMul exceeded its quantization bound: ratio {max_ratio:.3}"
    );
}

#[test]
fn test_fixed_div_against_exact_oracle() {
    let mut rng = Rng(0x5E70_D00D_0000_0002);
    let mut samples = 0u64;
    let mut skipped = 0u64;
    let mut max_abs_ratio = 0.0f64;
    let mut max_rel_error = 0.0f64;
    let mut max_c_divergence = 0.0f64;
    let mut exact_c_matches = 0u64;
    let mut saturation_agreements = 0u64;
    let mut saturation_cases = 0u64;
    let mut worst: Vec<(f64, i32, i32, i32, f64)> = Vec::new();

    let mut check = |a: i32, b: i32| {
        let ternary = setun_FixedDiv(a, b);
        let c = c_fixed_div(a, b);

        if b == 0 || (a.unsigned_abs() >> 14) >= b.unsigned_abs() {
            saturation_cases += 1;
            if ternary == c {
                saturation_agreements += 1;
            }
            return;
        }

        if !c_fixed_div_is_defined(a, b) {
            skipped += 1;
            return;
        }

        let real = (a as f64) * FRACUNIT / b as f64;
        let big_a = (a as f64).abs() / FRACUNIT;
        let big_b = (b as f64).abs() / FRACUNIT;

        // Input quantization of `a` divided by `b`, plus the derivative term for `b`,
        // plus the output quantization.
        let bound = (HALF_TERNARY_STEP / big_b
            + big_a * HALF_TERNARY_STEP / (big_b * big_b)
            + HALF_TERNARY_STEP)
            * FRACUNIT
            + 2.0;
        let abs_error = (ternary as f64 - real).abs();
        let ratio = abs_error / bound.max(f64::EPSILON);
        max_abs_ratio = max_abs_ratio.max(ratio);

        let c_divergence = (ternary - c).abs() as f64;
        max_c_divergence = max_c_divergence.max(c_divergence);
        if ternary == c {
            exact_c_matches += 1;
        }
        assert!(
            c_divergence <= bound + 1.0,
            "ternary FixedDiv diverges from C by {c_divergence:.3} units (a={a}, b={b})"
        );
        if real.abs() >= 1_000.0 && b.unsigned_abs() >= 16_384 {
            let rel_error = (abs_error / real).abs();
            max_rel_error = max_rel_error.max(rel_error);
            if rel_error > 1.0e-3 {
                worst.push((rel_error, a, b, ternary, real));
            }
        }
        samples += 1;
    };

    for &a in EDGES.iter() {
        for &b in EDGES.iter() {
            check(a, b);
        }
    }
    for _ in 0..2_000 {
        check(rng.next_i32(), rng.next_i32());
    }
    // Divisors at or above 0.25 where both the C 32-bit algorithm and the 3^-10 bridge stay
    // faithful; `a` is capped so the quotient fits the C `q << 16` step.
    for _ in 0..8_000 {
        let b = rng.next_in(16_384, 32_768) * if rng.next_u64() & 1 == 0 { 1 } else { -1 };
        // Stay below the saturation guard `(|a| >> 14) >= |b|`.
        let a_limit = ((b.unsigned_abs() as i64) << 14) - 1;
        check(rng.next_bounded(a_limit), b);
    }

    worst.sort_by(|x, y| y.0.total_cmp(&x.0));
    for (rel, a, b, ternary, real) in worst.iter().take(4) {
        println!("  div outlier: a={a} b={b} ternary={ternary} real={real:.3} rel={rel:.3e}");
    }
    println!(
        "FixedDiv: {samples} compared, {skipped} skipped (C overflow), max error/bound = {max_abs_ratio:.3}, \
         max relative error for |result| >= 1000 units = {max_rel_error:.3e}, \
         max divergence from C = {max_c_divergence:.3} units, exact C matches = \
         {exact_c_matches}/{samples}, saturation agreement = \
         {saturation_agreements}/{saturation_cases}"
    );

    assert!(samples > 7_000, "corpus must exercise the normal range");
    assert!(
        max_rel_error <= 1.0e-3,
        "ternary FixedDiv relative error {max_rel_error:.3e} exceeds 0.1% for divisors >= 0.25"
    );
    assert_eq!(
        saturation_agreements, saturation_cases,
        "saturation guard must match the C engine exactly"
    );
    assert!(
        max_abs_ratio <= 1.001,
        "ternary FixedDiv exceeded its quantization bound: ratio {max_abs_ratio:.3}"
    );
}
