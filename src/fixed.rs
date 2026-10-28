//! Balanced Ternary Fixed-Point Arithmetic (`TernaryFixed`).
//!
//! Replaces Doom binary `fixed_t` (`16.16`, scale $2^{16} = 65,536$)
//! with a symmetric partition in `Word21`:
//! - **11 integer trits** (integer range $[-88,573, +88,573]$).
//! - **10 fractional trits** (scale factor $3^{10} = 59,049$, resolution $\approx 1.69 \times 10^{-5}$).
//!
//! Under balanced ternary logic:
//! - Negation is purely symmetric (`-x`), eliminating two's-complement sign asymmetry.
//! - Truncation and rounding of `10` fractional trits by balanced ternary division
//!   naturally rounds to the nearest representable value, reducing accumulated drift
//!   in Doom BSP projection and raycasting calculations.

use crate::trit::Trit;
use crate::word::{wrap_balanced_i128, Word21, HALF_POW3, POW3};
use core::fmt;
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Number of trits reserved for the integer part in `Word21`.
pub const INT_TRITS: usize = 11;

/// Number of trits reserved for the fractional part in `Word21`.
pub const FRAC_TRITS: usize = 10;

/// Fractional ternary scale factor: $3^{10} = 59,049$ (ternary counterpart to `FRACUNIT`).
pub const FRAC_SCALE: i64 = POW3[FRAC_TRITS]; // 59_049

/// Half of fractional scale factor ($29,524$), maximum bound of balanced fractional portion.
pub const HALF_FRAC_SCALE: i64 = HALF_POW3[FRAC_TRITS]; // 29_524

/// Classic Doom 16.16 binary fixed unit (`FRACUNIT = 65536`).
pub const DOOM_BINARY_FRACUNIT: i64 = 65_536;

/// Balanced ternary fixed-point number (`11.10` trits in a `Word21`).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TernaryFixed(Word21);

impl TernaryFixed {
    /// Zero in fixed-point (`0.0`).
    pub const ZERO: Self = Self(Word21::ZERO);

    /// Positive unit in fixed-point (`+1.0` = $+3^{10} = +59,049$).
    pub const ONE: Self = Self(Word21::new(FRAC_SCALE));

    /// Negative unit in fixed-point (`-1.0` = $-3^{10} = -59,049$).
    pub const NEG_ONE: Self = Self(Word21::new(-FRAC_SCALE));

    /// Constructs a `TernaryFixed` directly from an already scaled raw `Word21`.
    #[must_use]
    #[inline(always)]
    pub const fn from_raw(word: Word21) -> Self {
        Self(word)
    }

    /// Constructs a `TernaryFixed` from a raw scaled integer (`raw_val`).
    #[must_use]
    #[inline(always)]
    pub const fn from_raw_i64(raw_val: i64) -> Self {
        Self(Word21::new(raw_val))
    }

    /// Returns the underlying raw `Word21`.
    #[must_use]
    #[inline(always)]
    pub const fn raw_word(self) -> Word21 {
        self.0
    }

    /// Returns the raw scaled value as `i64`.
    #[must_use]
    #[inline(always)]
    pub const fn raw_i64(self) -> i64 {
        self.0.as_i64()
    }

    /// Constructs a `TernaryFixed` from an integer (`n * 3^10`).
    #[must_use]
    #[inline(always)]
    pub const fn from_int(n: i64) -> Self {
        Self(Word21::new(n * FRAC_SCALE))
    }

    /// Extracts the integer part rounded according to balanced ternary partition
    /// (discarding 10 least significant fractional trits by physical wire shift)
    /// and the fractional portion in `[-29,524, +29,524]`.
    #[must_use]
    #[inline]
    pub fn split_balanced(self) -> (i64, i64) {
        let int_part = self.0.shift_trits(-10).as_i64();
        let frac_part = self.0.as_i64() - int_part * FRAC_SCALE;
        (int_part, frac_part)
    }

    /// Returns the integer part truncated toward zero (for C/Doom compatibility).
    #[must_use]
    #[inline(always)]
    pub const fn to_int_trunc(self) -> i64 {
        self.0.as_i64() / FRAC_SCALE
    }

    /// Returns the integer part by native 10-trit truncation (symmetric rounding).
    #[must_use]
    #[inline(always)]
    pub fn to_int_balanced(self) -> i64 {
        self.0.shift_trits(-10).as_i64()
    }

    /// Converts from `f64` to `11.10` ternary fixed-point.
    #[must_use]
    #[inline]
    pub fn from_f64(val: f64) -> Self {
        let x = val * (FRAC_SCALE as f64);
        let scaled = if x >= 0.0 {
            (x + 0.5) as i64
        } else {
            (x - 0.5) as i64
        };
        Self::from_raw_i64(scaled)
    }

    /// Converts `11.10` ternary fixed-point to `f64`.
    #[must_use]
    #[inline]
    pub fn to_f64(self) -> f64 {
        (self.0.as_i64() as f64) / (FRAC_SCALE as f64)
    }

    /// Converts a Doom `16.16` binary `fixed_t` (`i32`) to `TernaryFixed` (`11.10` trits).
    #[must_use]
    #[inline(always)]
    pub const fn from_doom_fixed16(doom_fixed: i32) -> Self {
        let num = (doom_fixed as i64) * FRAC_SCALE;
        let scaled = if num >= 0 {
            (num + (DOOM_BINARY_FRACUNIT / 2)) / DOOM_BINARY_FRACUNIT
        } else {
            (num - (DOOM_BINARY_FRACUNIT / 2)) / DOOM_BINARY_FRACUNIT
        };
        Self::from_raw_i64(scaled)
    }

    /// Converts `TernaryFixed` (`11.10` trits) back to Doom `16.16` binary `fixed_t` (`i32`).
    #[must_use]
    #[inline(always)]
    pub const fn to_doom_fixed16(self) -> i32 {
        let num = (self.0.as_i64() as i128) * (DOOM_BINARY_FRACUNIT as i128);
        let half = (FRAC_SCALE / 2) as i128;
        let scaled = if num >= 0 {
            (num + half) / (FRAC_SCALE as i128)
        } else {
            (num - half) / (FRAC_SCALE as i128)
        };
        if scaled > i32::MAX as i128 {
            i32::MAX
        } else if scaled < i32::MIN as i128 {
            i32::MIN
        } else {
            scaled as i32
        }
    }

    /// Direct trichotomous sign (`Negative`, `Zero`, `Positive`).
    #[inline(always)]
    pub const fn sign_trit(self) -> Trit {
        self.0.sign_trit()
    }

    /// Ternary fixed-point multiplication (`FixedMul` equivalent):
    /// Operates via physical `21x21 -> 42` trit gate multiplier (`gate_shift_and_add_mul42`)
    /// and 10-trit wire shifting (`prod42[10..31]`), performing exact symmetric rounding
    /// to nearest without extra adder stages.
    #[inline]
    pub fn fixed_mul(self, rhs: Self) -> Self {
        self.fixed_mul_with_rem(rhs).0
    }

    /// Fixed-point multiplication returning also the 10 least significant fractional trits
    /// (`prod42[0..10]`) discarded by shift, populating the CPU Multiplier/Quotient (`R`) register.
    #[inline]
    pub fn fixed_mul_with_rem(self, rhs: Self) -> (Self, Word21) {
        let (prod42, _) = self.0.gate_shift_and_add_mul42(rhs.0);
        let mut mid21 = [Trit::Zero; 21];
        mid21.copy_from_slice(&prod42[FRAC_TRITS..(FRAC_TRITS + 21)]);

        let mut low_rem = [Trit::Zero; 21];
        low_rem[0..FRAC_TRITS].copy_from_slice(&prod42[0..FRAC_TRITS]);

        let result_word = Word21::from_trits(&mid21);
        let rem_word = Word21::from_trits(&low_rem);

        // Strict mathematical check against silent divergence:
        let prod = (self.0.as_i64() as i128) * (rhs.0.as_i64() as i128);
        let scale = FRAC_SCALE as i128;
        let half = HALF_FRAC_SCALE as i128;
        let expected_shifted = if prod >= 0 {
            (prod + half) / scale
        } else {
            (prod - half) / scale
        };
        let expected_rem = prod - expected_shifted * scale;
        assert_eq!(
            result_word.as_i64(),
            wrap_balanced_i128::<21>(expected_shifted),
            "Divergence in 10-trit shift of TernaryFixed multiplier"
        );
        assert_eq!(
            rem_word.as_i64(),
            expected_rem as i64,
            "Divergence in 10-trit fractional remainder of TernaryFixed multiplier"
        );

        (Self(result_word), rem_word)
    }

    /// Ternary fixed-point division (`FixedDiv`) operating via non-restoring balanced
    /// algorithm with physical wire shift and `Word21::gate_ripple_carry_add` adder,
    /// without host CPU `/` division.
    /// Returns `Option<(quotient, carry_propagations)>` or `None` if `rhs == 0`.
    pub fn gate_fixed_div(self, rhs: Self) -> Option<(Self, u32)> {
        if rhs.0.as_i64() == 0 {
            return None;
        }
        let denom = rhs.0.as_i64() as i128;
        // Numerator scaled by 3^10 is a physical 10-trit left wire shift
        let mut rem = (self.0.as_i64() as i128) * (FRAC_SCALE as i128);
        let mut quot_trits = [Trit::Zero; 21];
        let mut total_carries = 0u32;

        for k in (0..=20).rev() {
            let p = POW3[k] as i128;
            let y_k = denom * p;
            if y_k == 0 {
                continue;
            }

            let two_rem = rem * 2;
            let abs_two_rem = two_rem.abs();
            let abs_yk = y_k.abs();

            if abs_two_rem >= abs_yk {
                let same_sign = (two_rem >= 0 && y_k > 0) || (two_rem < 0 && y_k < 0);
                let q_k_trit = if same_sign {
                    Trit::Positive
                } else {
                    Trit::Negative
                };
                let sub = (q_k_trit.as_i64() as i128) * y_k;
                let (_diff, _c, carries) = Word21::new(rem as i64)
                    .gate_ripple_carry_add(Word21::new(-(sub as i64)), Trit::Zero);
                total_carries += carries.max(1);
                rem -= sub;
                quot_trits[k] = q_k_trit;
            }
        }

        let quot = Word21::from_trits(&quot_trits);
        Some((Self(quot), total_carries))
    }

    /// Checked ternary fixed-point division (`FixedDiv`):
    /// Returns `None` if `rhs == 0` (silent failure prohibited).
    #[inline(always)]
    pub fn checked_fixed_div(self, rhs: Self) -> Option<Self> {
        self.gate_fixed_div(rhs).map(|(q, _)| q)
    }

    /// Ternary fixed-point division (`FixedDiv`):
    /// Computes `(self * 3^10) / rhs` and triggers an explicit panic if `rhs == 0`
    /// (use `checked_fixed_div` when capturing CPU division traps).
    #[inline]
    pub fn fixed_div(self, rhs: Self) -> Self {
        self.checked_fixed_div(rhs)
            .expect("Division by zero in TernaryFixed::fixed_div (silent failure prohibited)")
    }
}

impl Add for TernaryFixed {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for TernaryFixed {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0 + rhs.0;
    }
}

impl Sub for TernaryFixed {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for TernaryFixed {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0 - rhs.0;
    }
}

impl Mul for TernaryFixed {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self::Output {
        self.fixed_mul(rhs)
    }
}

impl Div for TernaryFixed {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Self) -> Self::Output {
        self.fixed_div(rhs)
    }
}

impl Neg for TernaryFixed {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl fmt::Display for TernaryFixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.5}", self.to_f64())
    }
}

impl From<i32> for TernaryFixed {
    #[inline(always)]
    fn from(n: i32) -> Self {
        Self::from_int(n as i64)
    }
}

impl From<i16> for TernaryFixed {
    #[inline(always)]
    fn from(n: i16) -> Self {
        Self::from_int(n as i64)
    }
}

impl From<i8> for TernaryFixed {
    #[inline(always)]
    fn from(n: i8) -> Self {
        Self::from_int(n as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_addition_and_negative_symmetry() {
        let a = TernaryFixed::from_f64(12.5);
        let b = TernaryFixed::from_f64(-7.25);
        let sum = a + b;
        let expected = TernaryFixed::from_f64(5.25);

        assert!((sum.to_f64() - expected.to_f64()).abs() < 1e-4);
        assert_eq!(a + (-a), TernaryFixed::ZERO);
        assert_eq!(-(a + b), (-a) + (-b));
    }

    #[test]
    fn test_fixed_multiplication_with_negatives() {
        let a = TernaryFixed::from_f64(3.5);
        let b = TernaryFixed::from_f64(-2.0);
        let prod1 = a * b;
        let prod2 = (-a) * b;
        let prod3 = (-a) * (-b);

        assert!((prod1.to_f64() - (-7.0)).abs() < 1e-4);
        // Strict symmetry: (-a) * b == -(a * b)
        assert_eq!(prod1, -prod2);
        // (-a) * (-b) == a * b
        assert_eq!(prod3, a * b);
    }

    #[test]
    fn test_fixed_division_and_doom_16_16_bridge() {
        let num = TernaryFixed::from_int(-45);
        let den = TernaryFixed::from_int(6);
        let q = num / den;
        assert!((q.to_f64() - (-7.5)).abs() < 1e-4);

        // Round-trip conversion to Doom fixed_t 16.16
        let doom_val: i32 = -(42 * 65536 + 32768); // -42.5 in 16.16
        let tf = TernaryFixed::from_doom_fixed16(doom_val);
        assert!((tf.to_f64() - (-42.5)).abs() < 1e-4);
        let back = tf.to_doom_fixed16();
        assert!((back - doom_val).abs() <= 2);
    }

    #[test]
    fn test_property_algebraic_symmetry_and_identities() {
        // Deterministic sequence of samples across varied ranges
        let samples: [f64; 10] = [
            0.0, 1.0, -1.0, 0.5, -0.5, 17.125, -42.875, 1023.0, -512.25, 0.0001,
        ];

        for &val_a in &samples {
            let a = TernaryFixed::from_f64(val_a);

            // Neutral element and additive inverse identities
            assert_eq!(a + TernaryFixed::ZERO, a);
            assert_eq!(a + (-a), TernaryFixed::ZERO);

            for &val_b in &samples {
                let b = TernaryFixed::from_f64(val_b);

                // Commutativity of Addition: a + b == b + a
                assert_eq!(a + b, b + a);

                // Commutativity of Multiplication: a * b == b * a
                assert_eq!(a * b, b * a);

                // Strict Inversion Symmetry: (-a) * b == -(a * b)
                assert_eq!((-a) * b, -(a * b));

                // Double Inversion: (-a) * (-b) == a * b
                assert_eq!((-a) * (-b), a * b);
            }
        }
    }
}
