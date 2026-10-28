// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Oracle comparison for the BSP geometry hooks.
//!
//! Compares `setun_PointOnSide` (Doom `R_PointOnSide` / `R_PointOnSegSide`) and
//! `setun_AproxDistance` (Doom `P_AproxDistance`) against faithful transcriptions of the vendored
//! C code in `r_main.c` and `p_maputl.c`.
//!
//! Both hooks run inside the ternary CPU through firmware ROM routines, so the comparison covers
//! the assembled programs, not only the Rust wrapper.

use setun_doom::mmo::{setun_AproxDistance, setun_PointOnSide};

/// Doom `FRACUNIT` for 16.16 fixed point.
const FRACUNIT: i64 = 65_536;

/// Faithful transcription of the vendored `FixedMul` split algorithm.
fn c_fixed_mul(a: i32, b: i32) -> i32 {
    let ahi = a >> 16;
    let alo = a & 0xffff;
    ahi.wrapping_mul(b).wrapping_add(alo.wrapping_mul(b) >> 16)
}

/// Returns true when the C split `FixedMul` stays inside `i32` at every step.
fn c_fixed_mul_is_defined(a: i32, b: i32) -> bool {
    let ahi = (a >> 16) as i64;
    let alo = (a & 0xffff) as i64;
    let high = ahi * (b as i64);
    let low = alo * (b as i64);
    let shifted = low >> 16;
    let fits = |v: i64| (i32::MIN as i64..=i32::MAX as i64).contains(&v);
    fits(high) && fits(low) && fits(high + shifted)
}

/// Faithful transcription of `R_PointOnSide` (`r_main.c`, `#else` branch).
fn c_point_on_side(x: i32, y: i32, lx: i32, ly: i32, ldx: i32, ldy: i32) -> i32 {
    if ldx == 0 {
        return if x <= lx {
            (ldy > 0) as i32
        } else {
            (ldy < 0) as i32
        };
    }
    if ldy == 0 {
        return if y <= ly {
            (ldx < 0) as i32
        } else {
            (ldx > 0) as i32
        };
    }

    let dx = x.wrapping_sub(lx);
    let dy = y.wrapping_sub(ly);

    if (ldy ^ ldx ^ dx ^ dy) & i32::MIN != 0 {
        return if (ldy ^ dx) & i32::MIN != 0 { 1 } else { 0 };
    }

    let left = c_fixed_mul(ldy >> 16, dx);
    let right = c_fixed_mul(dy, ldx >> 16);
    if right < left {
        0
    } else {
        1
    }
}

/// Returns true when the C `R_PointOnSide` path stays inside `i32` for the split multiplies.
fn c_point_on_side_is_defined(x: i32, y: i32, lx: i32, ly: i32, ldx: i32, ldy: i32) -> bool {
    if ldx == 0 || ldy == 0 {
        return true;
    }
    let dx = x.wrapping_sub(lx);
    let dy = y.wrapping_sub(ly);
    if (ldy ^ ldx ^ dx ^ dy) & i32::MIN != 0 {
        return true;
    }
    c_fixed_mul_is_defined(ldy >> 16, dx) && c_fixed_mul_is_defined(dy, ldx >> 16)
}

/// Faithful transcription of `P_AproxDistance` (`p_maputl.c`, `#else` branch).
fn c_aprox_distance(dx: i32, dy: i32) -> i32 {
    let dx = dx.wrapping_abs();
    let dy = dy.wrapping_abs();
    if dx < dy {
        dx.wrapping_add(dy).wrapping_sub(dx >> 1)
    } else {
        dx.wrapping_add(dy).wrapping_sub(dy >> 1)
    }
}

/// One mismatching sample: point, origin, delta, ternary side, C side.
type SideMismatch = (i32, i32, i32, i32, i32, i32, i32, i32);

/// Deterministic xorshift64* generator, so the corpus is reproducible across runs.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn next_in(&mut self, lo: i64, hi: i64) -> i32 {
        let span = hi - lo + 1;
        (lo + (self.next_u64() % span as u64) as i64) as i32
    }
}

#[test]
fn test_point_on_side_against_c_oracle() {
    let mut rng = Rng(0x5e70_d00d_0000_0003);
    let mut compared = 0u64;
    let mut mismatches = 0u64;
    let mut exact_ties = 0u64;
    let mut band_ties = 0u64;
    let mut far_mismatches = 0u64;
    let mut axis_aligned = 0u64;
    let mut on_plane = 0u64;
    let mut examples: Vec<SideMismatch> = Vec::new();

    let mut check = |x: i32, y: i32, lx: i32, ly: i32, ldx: i32, ldy: i32| {
        if !c_point_on_side_is_defined(x, y, lx, ly, ldx, ldy) {
            return;
        }
        let ternary = setun_PointOnSide(x, y, lx, ly, ldx, ldy);
        let c = c_point_on_side(x, y, lx, ly, ldx, ldy);
        compared += 1;
        if ldx == 0 || ldy == 0 {
            axis_aligned += 1;
        }
        if ternary != c {
            mismatches += 1;
            // Exact cross product of the unrounded inputs; a mismatch is only acceptable when the
            // point sits on the plane or inside the rounding band of it.
            let dx = (x as i128) - (lx as i128);
            let dy = (y as i128) - (ly as i128);
            let cross = (ldx as i128) * dy - (ldy as i128) * dx;
            let band = (ldx.unsigned_abs() as i128 + ldy.unsigned_abs() as i128) + 2;
            if cross == 0 {
                exact_ties += 1;
            } else if cross.abs() <= band {
                band_ties += 1;
            } else {
                far_mismatches += 1;
                if examples.len() < 8 {
                    examples.push((x, y, lx, ly, ldx, ldy, ternary, c));
                }
            }
        }
    };

    // Map-scale geometry: coordinates up to +-4096 map units in 16.16.
    let map = 4096 * FRACUNIT;
    for _ in 0..9_000 {
        let lx = rng.next_in(-map, map);
        let ly = rng.next_in(-map, map);
        let ldx = rng.next_in(-map, map);
        let ldy = rng.next_in(-map, map);
        let x = rng.next_in(-map, map);
        let y = rng.next_in(-map, map);
        check(x, y, lx, ly, ldx, ldy);
    }

    // Axis-aligned partitions, where the C path has dedicated branches.
    for _ in 0..500 {
        let lx = rng.next_in(-map, map);
        let ly = rng.next_in(-map, map);
        let x = rng.next_in(-map, map);
        let y = rng.next_in(-map, map);
        check(x, y, lx, ly, 0, rng.next_in(-map, map));
        check(x, y, lx, ly, rng.next_in(-map, map), 0);
    }

    // Points exactly on the partition line: ties are the sensitive case.
    for _ in 0..500 {
        let lx = rng.next_in(-map / 2, map / 2);
        let ly = rng.next_in(-map / 2, map / 2);
        let ldx = rng.next_in(-map / 2, map / 2);
        let ldy = rng.next_in(-map / 2, map / 2);
        let t = rng.next_in(-8, 8);
        let x = lx.saturating_add(ldx / 8 * t);
        let y = ly.saturating_add(ldy / 8 * t);
        on_plane += 1;
        check(x, y, lx, ly, ldx, ldy);
    }

    for (x, y, lx, ly, ldx, ldy, ternary, c) in examples.iter() {
        println!(
            "  side mismatch: p=({x},{y}) origin=({lx},{ly}) delta=({ldx},{ldy}) ternary={ternary} c={c}"
        );
    }
    println!(
        "PointOnSide: {compared} compared ({axis_aligned} axis-aligned, {on_plane} on-plane), \
         mismatches = {mismatches} ({:.3}%): {exact_ties} exactly on the plane, {band_ties} inside \
         the rounding band, {far_mismatches} away from it",
        100.0 * mismatches as f64 / compared.max(1) as f64
    );

    assert!(compared > 5_000, "corpus must exercise map-scale geometry");
    assert_eq!(
        far_mismatches, 0,
        "ternary side must match the C classifier away from the plane"
    );
}

#[test]
fn test_aprox_distance_against_c_oracle() {
    let mut rng = Rng(0x5e70_d00d_0000_0004);
    let mut compared = 0u64;
    let mut max_diff = 0i64;
    let mut exact = 0u64;
    let mut examples: Vec<(i32, i32, i32, i32)> = Vec::new();

    let mut max_ratio = 0.0f64;
    let mut check = |dx: i32, dy: i32| {
        if dx == i32::MIN || dy == i32::MIN {
            return;
        }
        // The C sum overflows i32 outside this range; the ternary path saturates instead.
        if (dx.unsigned_abs() as u64) + (dy.unsigned_abs() as u64) > i32::MAX as u64 {
            return;
        }
        let ternary = setun_AproxDistance(dx, dy);
        let c = c_aprox_distance(dx, dy);
        let diff = (ternary as i64 - c as i64).abs();
        // The ROM halves the smaller magnitude in ternary raw units, while Doom uses `>> 1`
        // on 16.16; converting between scales costs at most one unit on each side.
        max_ratio = max_ratio.max(diff as f64 / 2.0);
        compared += 1;
        if diff == 0 {
            exact += 1;
        } else if diff > max_diff {
            max_diff = diff;
            if examples.len() < 8 {
                examples.push((dx, dy, ternary, c));
            }
        }
    };

    let map = 4096 * FRACUNIT;
    for _ in 0..4_000 {
        check(rng.next_in(-map, map), rng.next_in(-map, map));
    }
    for _ in 0..1_000 {
        check(rng.next_in(-2, 2), rng.next_in(-2, 2));
    }
    for &(dx, dy) in &[
        (0, 0),
        (FRACUNIT as i32, 0),
        (0, FRACUNIT as i32),
        (FRACUNIT as i32, FRACUNIT as i32),
        (-(FRACUNIT as i32), FRACUNIT as i32),
        (1, 1),
        (1, -1),
        (i32::MAX, i32::MAX),
        (i32::MIN + 1, i32::MIN + 1),
    ] {
        check(dx, dy);
    }

    for (dx, dy, ternary, c) in examples.iter() {
        println!("  distance diff: dx={dx} dy={dy} ternary={ternary} c={c}");
    }
    println!(
        "AproxDistance: {compared} compared, exact = {exact} ({:.2}%), max |diff| = {max_diff} units",
        100.0 * exact as f64 / compared.max(1) as f64
    );

    assert!(compared > 4_000, "corpus must exercise map-scale distances");
    assert!(
        max_diff <= 2,
        "ternary AproxDistance differs from C by more than two 2^-16 units"
    );
    assert!(
        exact * 2 > compared,
        "more than half of the samples should match the C approximation exactly"
    );
}
