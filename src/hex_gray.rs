// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Hexagonal Gray Code (`HexGray3`) on the 3-bit Boolean Hypercube $Q_3$.
//!
//! Encodes the 7 zero-sum balanced ternary hexagonal unit states
//! $(q, r, s) \in \{-1, 0, +1\}^3$ with $q + r + s = 0$ plus an explicit hardware
//! fault sentinel (`0b100`) into 3 binary wires:
//!
//! - `3'b000` -> `( 0,  0,  0)` Center / Neutral
//! - `3'b001` -> `(+1,  0, -1)` Ring $v_0$ ($0^\circ$)
//! - `3'b011` -> `( 0, +1, -1)` Ring $v_1$ ($60^\circ$)
//! - `3'b010` -> `(-1, +1,  0)` Ring $v_2$ ($120^\circ$)
//! - `3'b110` -> `(-1,  0, +1)` Ring $v_3$ ($180^\circ$)
//! - `3'b111` -> `( 0, -1, +1)` Ring $v_4$ ($240^\circ$)
//! - `3'b101` -> `(+1, -1,  0)` Ring $v_5$ ($300^\circ$)
//! - `3'b100` -> Sentinel ($\bot$) Fault / Parity trap

use crate::trit::Trit;

/// 3-bit Hexagonal Gray code representing a zero-sum balanced ternary unit cell
/// $(q, r, s)$ with $q + r + s = 0$ or the fault sentinel `0b100`.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct HexGray3(u8);

impl HexGray3 {
    /// Center / origin `(0, 0, 0)`.
    pub const CENTER: Self = Self(0b000);
    /// Ring vertex $v_0$ `(+1, 0, -1)` at $0^\circ$.
    pub const V0: Self = Self(0b001);
    /// Ring vertex $v_1$ `(0, +1, -1)` at $60^\circ$.
    pub const V1: Self = Self(0b011);
    /// Ring vertex $v_2$ `(-1, +1, 0)` at $120^\circ$.
    pub const V2: Self = Self(0b010);
    /// Ring vertex $v_3$ `(-1, 0, +1)` at $180^\circ$.
    pub const V3: Self = Self(0b110);
    /// Ring vertex $v_4$ `(0, -1, +1)` at $240^\circ$.
    pub const V4: Self = Self(0b111);
    /// Ring vertex $v_5$ `(+1, -1, 0)` at $300^\circ$.
    pub const V5: Self = Self(0b101);
    /// Hardware fault / parity sentinel `3'b100`.
    pub const SENTINEL: Self = Self(0b100);

    /// Cyclic 6-ring order (`CCW` +60 degree traversal).
    pub const RING_C6: [Self; 6] = [Self::V0, Self::V1, Self::V2, Self::V3, Self::V4, Self::V5];

    /// Hamiltonian Gray path through all 7 non-sentinel vertices of $Q_3$ (`d_H = 1` per step),
    /// indexing symmetric signed levels `[-3, -2, -1, 0, +1, +2, +3]`.
    pub const LINEAR_7: [Self; 7] = [
        Self::V5,     // -3 (101)
        Self::V4,     // -2 (111)
        Self::V3,     // -1 (110)
        Self::V2,     //  0 (010)
        Self::CENTER, // +1 (000)
        Self::V0,     // +2 (001)
        Self::V1,     // +3 (011)
    ];

    /// Creates a `HexGray3` masking to the 3 physical wire bits.
    #[inline(always)]
    #[must_use]
    pub const fn from_raw(bits: u8) -> Self {
        Self(bits & 0b111)
    }

    /// Returns the raw 3-bit physical wire representation (`0..=7`).
    #[inline(always)]
    #[must_use]
    pub const fn as_raw(self) -> u8 {
        self.0
    }

    /// Returns `true` if the cell holds the reserved fault sentinel `3'b100`.
    #[inline(always)]
    #[must_use]
    pub const fn is_sentinel(self) -> bool {
        self.0 == Self::SENTINEL.0
    }

    /// Encodes balanced ternary zero-sum coordinates `(q, r, s)` into `HexGray3`.
    /// Returns `None` if `q + r + s != 0`.
    #[must_use]
    pub const fn from_trits(q: Trit, r: Trit, s: Trit) -> Option<Self> {
        match (q, r, s) {
            (Trit::Zero, Trit::Zero, Trit::Zero) => Some(Self::CENTER),
            (Trit::Positive, Trit::Zero, Trit::Negative) => Some(Self::V0),
            (Trit::Zero, Trit::Positive, Trit::Negative) => Some(Self::V1),
            (Trit::Negative, Trit::Positive, Trit::Zero) => Some(Self::V2),
            (Trit::Negative, Trit::Zero, Trit::Positive) => Some(Self::V3),
            (Trit::Zero, Trit::Negative, Trit::Positive) => Some(Self::V4),
            (Trit::Positive, Trit::Negative, Trit::Zero) => Some(Self::V5),
            _ => None,
        }
    }

    /// Decodes `HexGray3` into `(q, r, s)` zero-sum balanced trits, or `None` for `SENTINEL`.
    #[must_use]
    pub const fn to_trits(self) -> Option<[Trit; 3]> {
        match self.0 {
            0b000 => Some([Trit::Zero, Trit::Zero, Trit::Zero]),
            0b001 => Some([Trit::Positive, Trit::Zero, Trit::Negative]),
            0b011 => Some([Trit::Zero, Trit::Positive, Trit::Negative]),
            0b010 => Some([Trit::Negative, Trit::Positive, Trit::Zero]),
            0b110 => Some([Trit::Negative, Trit::Zero, Trit::Positive]),
            0b111 => Some([Trit::Zero, Trit::Negative, Trit::Positive]),
            0b101 => Some([Trit::Positive, Trit::Negative, Trit::Zero]),
            _ => None,
        }
    }

    /// Encodes a signed scalar in `[-3, +3]` onto the 7-state `d_H = 1` Hamiltonian path.
    /// Out-of-range inputs return `Self::SENTINEL`.
    #[must_use]
    pub const fn from_signed_7(val: i8) -> Self {
        if val < -3 || val > 3 {
            Self::SENTINEL
        } else {
            Self::LINEAR_7[(val + 3) as usize]
        }
    }

    /// Counter-clockwise (`+60` degree) next-state transition `delta_ccw`.
    /// Guaranteed `d_H(self, self.ccw()) == 1` across all 8 states (`0..=7`).
    #[inline(always)]
    #[must_use]
    pub const fn ccw(self) -> Self {
        match self.0 {
            0b001 => Self::V1, // 001 -> 011
            0b011 => Self::V2, // 011 -> 010
            0b010 => Self::V3, // 010 -> 110
            0b110 => Self::V4, // 110 -> 111
            0b111 => Self::V5, // 111 -> 101
            0b101 => Self::V0, // 101 -> 001
            0b000 => Self::V0, // 000 -> 001
            _ => Self::V5,     // 100 -> 101
        }
    }

    /// Clockwise (`-60` degree) next-state transition `delta_cw`.
    /// Guaranteed `d_H(self, self.cw()) == 1` across all 8 states (`0..=7`).
    #[inline(always)]
    #[must_use]
    pub const fn cw(self) -> Self {
        match self.0 {
            0b001 => Self::V5, // 001 -> 101
            0b101 => Self::V4, // 101 -> 111
            0b111 => Self::V3, // 111 -> 110
            0b110 => Self::V2, // 110 -> 010
            0b010 => Self::V1, // 010 -> 011
            0b011 => Self::V0, // 011 -> 001
            0b000 => Self::V2, // 000 -> 010
            _ => Self::V3,     // 100 -> 110
        }
    }

    /// Advances by a balanced `Trit` direction (`+1` = CCW, `-1` = CW, `0` = hold).
    #[inline(always)]
    #[must_use]
    pub const fn step(self, dir: Trit) -> Self {
        match dir {
            Trit::Positive => self.ccw(),
            Trit::Negative => self.cw(),
            Trit::Zero => self,
        }
    }

    /// Physical wire Hamming distance `d_H(self, next)` in `0..=3`.
    #[inline(always)]
    #[must_use]
    pub const fn wire_flips_to(self, next: Self) -> u32 {
        (self.0 ^ next.0).count_ones()
    }

    /// Packs the 3 zero-sum trits into Canonical 2-bit/trit representation (`0=00, +1=01, -1=10`, 6 wires).
    #[must_use]
    pub const fn to_canonical_6b(self) -> u8 {
        match self.to_trits() {
            Some([q, r, s]) => q.to_dual_bits() | (r.to_dual_bits() << 2) | (s.to_dual_bits() << 4),
            None => 0b11_11_11,
        }
    }

    /// Packs the 3 zero-sum trits into Offset 2-bit/trit representation (`-1=00, 0=01, +1=10`, 6 wires).
    #[must_use]
    pub const fn to_offset_6b(self) -> u8 {
        const fn off(t: Trit) -> u8 {
            match t {
                Trit::Negative => 0b00,
                Trit::Zero => 0b01,
                Trit::Positive => 0b10,
            }
        }
        match self.to_trits() {
            Some([q, r, s]) => off(q) | (off(r) << 2) | (off(s) << 4),
            None => 0b11_11_11,
        }
    }

    /// Packs the 3 zero-sum trits into One-Hot 3-bit/trit representation (`-1=100, 0=001, +1=010`, 9 wires).
    #[must_use]
    pub const fn to_onehot_9b(self) -> u16 {
        const fn oh(t: Trit) -> u16 {
            match t {
                Trit::Zero => 0b001,
                Trit::Positive => 0b010,
                Trit::Negative => 0b100,
            }
        }
        match self.to_trits() {
            Some([q, r, s]) => oh(q) | (oh(r) << 3) | (oh(s) << 6),
            None => 0,
        }
    }

    /// Packs the 7 geometric states into 7-bit One-Hot (`7` wires, sentinel = `0`).
    #[must_use]
    pub const fn to_onehot_7b(self) -> u8 {
        match self.0 {
            0b000 => 1 << 0,
            0b001 => 1 << 1,
            0b011 => 1 << 2,
            0b010 => 1 << 3,
            0b110 => 1 << 4,
            0b111 => 1 << 5,
            0b101 => 1 << 6,
            _ => 0,
        }
    }

    /// Counts logical ternary `(total, half_swings, full_swings)` between `self` and `next`
    /// on the underlying `(q, r, s)` trits.
    #[must_use]
    pub const fn trit_swings_to(self, next: Self) -> (u32, u32, u32) {
        let (Some(a), Some(b)) = (self.to_trits(), next.to_trits()) else {
            return (0, 0, 0);
        };
        let mut total = 0;
        let mut half = 0;
        let mut full = 0;
        let mut i = 0;
        while i < 3 {
            let diff = (a[i].as_i8() - b[i].as_i8()).unsigned_abs();
            if diff == 1 {
                total += 1;
                half += 1;
            } else if diff == 2 {
                total += 1;
                full += 1;
            }
            i += 1;
        }
        (total, half, full)
    }
}

/// Physical wire and swing telemetry across all five encodings of the same zero-sum state stream.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct HexCodingComparison {
    pub transitions: u64,
    pub trit_flips: u64,
    pub trit_half_swings: u64,
    pub trit_full_swings: u64,
    pub hex_gray3_wire_flips: u64,
    pub hex_gray3_glitch_steps: u64,
    pub canonical_6b_wire_flips: u64,
    pub canonical_6b_glitch_steps: u64,
    pub offset_6b_wire_flips: u64,
    pub offset_6b_glitch_steps: u64,
    pub onehot_9b_wire_flips: u64,
    pub onehot_9b_glitch_steps: u64,
    pub onehot_7b_wire_flips: u64,
    pub onehot_7b_glitch_steps: u64,
}

impl HexCodingComparison {
    /// Records one transition `prev -> next` across all encodings simultaneously.
    pub fn record(&mut self, prev: HexGray3, next: HexGray3) {
        if prev == next {
            return;
        }
        self.transitions += 1;
        let (t, h, f) = prev.trit_swings_to(next);
        self.trit_flips += u64::from(t);
        self.trit_half_swings += u64::from(h);
        self.trit_full_swings += u64::from(f);

        let hg = prev.wire_flips_to(next);
        self.hex_gray3_wire_flips += u64::from(hg);
        if hg > 1 {
            self.hex_gray3_glitch_steps += 1;
        }

        let can = (prev.to_canonical_6b() ^ next.to_canonical_6b()).count_ones();
        self.canonical_6b_wire_flips += u64::from(can);
        if can > 1 {
            self.canonical_6b_glitch_steps += 1;
        }

        let off = (prev.to_offset_6b() ^ next.to_offset_6b()).count_ones();
        self.offset_6b_wire_flips += u64::from(off);
        if off > 1 {
            self.offset_6b_glitch_steps += 1;
        }

        let oh9 = (prev.to_onehot_9b() ^ next.to_onehot_9b()).count_ones();
        self.onehot_9b_wire_flips += u64::from(oh9);
        if oh9 > 1 {
            self.onehot_9b_glitch_steps += 1;
        }

        let oh7 = (prev.to_onehot_7b() ^ next.to_onehot_7b()).count_ones();
        self.onehot_7b_wire_flips += u64::from(oh7);
        if oh7 > 1 {
            self.onehot_7b_glitch_steps += 1;
        }
    }

    /// Relative physical wire flip reduction of `HexGray3` against another wire flip total.
    #[must_use]
    pub fn reduction_vs(&self, baseline_wire_flips: u64) -> f64 {
        if baseline_wire_flips == 0 {
            return 0.0;
        }
        let base = baseline_wire_flips as f64;
        let hg = self.hex_gray3_wire_flips as f64;
        ((base - hg) / base) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_8_states_unit_hamming_in_both_directions() {
        for raw in 0u8..8 {
            let s = HexGray3::from_raw(raw);
            assert_eq!(
                s.wire_flips_to(s.ccw()),
                1,
                "CCW must flip 1 bit from {raw:03b}"
            );
            assert_eq!(
                s.wire_flips_to(s.cw()),
                1,
                "CW must flip 1 bit from {raw:03b}"
            );
        }
    }

    #[test]
    fn test_c6_ring_inverse_and_zero_sum_invariant() {
        let mut cur = HexGray3::V0;
        for expected in HexGray3::RING_C6 {
            assert_eq!(cur, expected);
            let [q, r, s] = cur.to_trits().expect("ring state valid");
            assert_eq!(q.as_i8() + r.as_i8() + s.as_i8(), 0);
            assert_eq!(HexGray3::from_trits(q, r, s), Some(cur));
            assert_eq!(cur.ccw().cw(), cur);
            assert_eq!(cur.cw().ccw(), cur);
            cur = cur.ccw();
        }
        assert_eq!(cur, HexGray3::V0);
        assert!(HexGray3::SENTINEL.is_sentinel());
        assert_eq!(HexGray3::SENTINEL.to_trits(), None);
    }

    #[test]
    fn test_linear7_hamiltonian_gray_path() {
        for pair in HexGray3::LINEAR_7.windows(2) {
            assert_eq!(pair[0].wire_flips_to(pair[1]), 1);
        }
        assert_eq!(HexGray3::from_signed_7(0), HexGray3::V2);
        assert_eq!(HexGray3::from_signed_7(4), HexGray3::SENTINEL);
    }

    #[test]
    fn test_ring_step_savings_over_canonical_offset_onehot() {
        let mut cmp = HexCodingComparison::default();
        let mut cur = HexGray3::V0;
        for _ in 0..60 {
            let next = cur.ccw();
            cmp.record(cur, next);
            cur = next;
        }
        assert_eq!(cmp.hex_gray3_wire_flips, 60);
        assert_eq!(cmp.hex_gray3_glitch_steps, 0);
        assert_eq!(cmp.canonical_6b_wire_flips, 120);
        assert_eq!(cmp.offset_6b_wire_flips, 180);
        assert_eq!(cmp.onehot_9b_wire_flips, 240);
        assert_eq!(cmp.onehot_7b_wire_flips, 120);
        assert_eq!(cmp.trit_full_swings, 0);
        assert_eq!(cmp.trit_half_swings, 120);
    }
}
