//! Balanced ternary representation and logic (Trits in set {-1, 0, +1}).
//!
//! Inspired by the electromagnetic architecture of the Setun (1958, Nikolai Brusentsov),
//! each trit assumes exactly one of three states symmetric around zero.

use core::fmt;
use core::ops::{BitAnd, BitOr, BitXor, Mul, Neg, Not};

/// Elementary state of a balanced ternary digit (Trit).
#[repr(i8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Trit {
    /// Negative state (-1), historically denoted by `'T'` or `'-'`.
    Negative = -1,
    /// Neutral state (0), denoted by `'0'`.
    #[default]
    Zero = 0,
    /// Positive state (+1), denoted by `'+'` or `'1'`.
    Positive = 1,
}

impl Trit {
    /// Converts a signed integer (`i8`) to `Trit` saturating to the range `[-1, +1]`.
    #[inline(always)]
    pub const fn from_i8(val: i8) -> Self {
        if val < 0 {
            Self::Negative
        } else if val > 0 {
            Self::Positive
        } else {
            Self::Zero
        }
    }

    /// Converts an integer (`i64`) to `Trit` by exact sign or balanced remainder.
    #[inline(always)]
    pub const fn from_i64_sign(val: i64) -> Self {
        if val < 0 {
            Self::Negative
        } else if val > 0 {
            Self::Positive
        } else {
            Self::Zero
        }
    }

    /// Returns the numeric value as `i8` (-1, 0, or 1).
    #[inline(always)]
    pub const fn as_i8(self) -> i8 {
        self as i8
    }

    /// Returns the numeric value as `i64` (-1, 0, or 1).
    #[inline(always)]
    pub const fn as_i64(self) -> i64 {
        self as i64
    }

    /// Ternary inversion (symmetric negation): -1 <-> +1, 0 -> 0.
    #[inline(always)]
    pub const fn invert(self) -> Self {
        match self {
            Self::Negative => Self::Positive,
            Self::Zero => Self::Zero,
            Self::Positive => Self::Negative,
        }
    }

    /// Ternary conjunction (Kleene/Lukasiewicz MIN): pure logic gate without binary arithmetic.
    #[inline(always)]
    pub const fn t_and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Negative, _) | (_, Self::Negative) => Self::Negative,
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (Self::Positive, Self::Positive) => Self::Positive,
        }
    }

    /// Ternary disjunction (Kleene/Lukasiewicz MAX): pure logic gate without binary arithmetic.
    #[inline(always)]
    pub const fn t_or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Positive, _) | (_, Self::Positive) => Self::Positive,
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (Self::Negative, Self::Negative) => Self::Negative,
        }
    }

    /// Ternary multiplication (isomorphic to balanced ternary XOR / Setun sign modulator):
    /// implemented via pure state switching without binary multiplication.
    #[inline(always)]
    pub const fn t_mul(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (Self::Positive, x) => x,
            (Self::Negative, x) => x.invert(),
        }
    }

    /// Ternary Consensus gate (half-adder `carry`):
    /// Returns `Positive` if both are `Positive`, `Negative` if both are `Negative`,
    /// and `Zero` otherwise.
    #[inline(always)]
    pub const fn consensus(self, other: Self) -> Self {
        match (self, other) {
            (Self::Positive, Self::Positive) => Self::Positive,
            (Self::Negative, Self::Negative) => Self::Negative,
            _ => Self::Zero,
        }
    }

    /// Sum-Modulo-3 gate (balanced ternary half-adder `sum`):
    /// Operates purely via state transitions in the ring `{-1, 0, +1}`.
    #[inline(always)]
    pub const fn sum_mod3(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, x) | (x, Self::Zero) => x,
            (Self::Positive, Self::Positive) => Self::Negative,
            (Self::Negative, Self::Negative) => Self::Positive,
            (Self::Positive, Self::Negative) | (Self::Negative, Self::Positive) => Self::Zero,
        }
    }

    /// Balanced ternary 1-trit half adder constructed from `sum_mod3` and `consensus` gates.
    /// Returns `(sum, carry_out)`.
    #[inline(always)]
    pub const fn half_add(self, other: Self) -> (Self, Self) {
        (self.sum_mod3(other), self.consensus(other))
    }

    /// Ternary Majority Vote (`majv.t` from REBEL-6 paper, Table II):
    /// If at least 2 of the 3 trits `(self, b, c)` share state `n`, returns `n`;
    /// if all 3 are distinct (`-1, 0, +1`), returns `Trit::Zero`.
    #[inline(always)]
    pub const fn majority_vote(self, b: Self, c: Self) -> Self {
        match (self, b, c) {
            (x, y, _) if x as i8 == y as i8 => x,
            (x, _, z) if x as i8 == z as i8 => x,
            (_, y, z) if y as i8 == z as i8 => y,
            _ => Self::Zero,
        }
    }

    /// Ternary Minority Vote (`minv.t` from REBEL-6 paper, Table II):
    /// When two trits are equal and one is unique, returns the unique trit (`cnt(n) == 1`).
    /// If all three are equal or all three are distinct (`-1, 0, +1`), returns `Trit::Zero`.
    #[inline(always)]
    pub const fn minority_vote(self, b: Self, c: Self) -> Self {
        match (self, b, c) {
            (x, y, z) if x as i8 == y as i8 && x as i8 != z as i8 => z,
            (x, y, z) if x as i8 == z as i8 && x as i8 != y as i8 => y,
            (x, y, z) if y as i8 == z as i8 && y as i8 != x as i8 => x,
            _ => Self::Zero,
        }
    }

    /// Trit Threshold Addition (`SAT` / Basic Operation `B21` `000+-+` of Setun-70, Brusentsov 1970):
    /// Linear threshold addition gate between two trits `S` (`self`) and `T` (`other`):
    /// returns `Positive (+1)` if `S + T > 0`, `Negative (-1)` if `S + T < 0`, and `Zero (0)` if `S + T == 0`.
    #[inline(always)]
    pub const fn threshold_add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Positive, Self::Positive)
            | (Self::Positive, Self::Zero)
            | (Self::Zero, Self::Positive) => Self::Positive,
            (Self::Negative, Self::Negative)
            | (Self::Negative, Self::Zero)
            | (Self::Zero, Self::Negative) => Self::Negative,
            (Self::Zero, Self::Zero)
            | (Self::Positive, Self::Negative)
            | (Self::Negative, Self::Positive) => Self::Zero,
        }
    }

    /// Ternary Masked Inhibition / Coalescence (`SIT` / Basic Operation `B20` `000+-0` of Setun-70):
    /// Algebraically defined by Brusentsov as `S + T * (1 - |S|)`:
    /// preserves `self` (`S`) if `self != Zero`; if `self == Zero`, adopts the state of `other` (`T`).
    #[inline(always)]
    pub const fn masked_inhibit(self, other: Self) -> Self {
        match self {
            Self::Zero => other,
            non_zero => non_zero,
        }
    }

    /// Elementary 1-trit trichotomous comparison (`cmp.t`):
    /// Returns `Positive (+1)` if `self > other`, `Zero (0)` if `self == other`, `Negative (-1)` if `self < other`.
    #[inline(always)]
    pub const fn cmp_trit(self, other: Self) -> Self {
        match (self, other) {
            (Self::Positive, Self::Zero)
            | (Self::Positive, Self::Negative)
            | (Self::Zero, Self::Negative) => Self::Positive,
            (Self::Negative, Self::Zero)
            | (Self::Negative, Self::Positive)
            | (Self::Zero, Self::Positive) => Self::Negative,
            _ => Self::Zero,
        }
    }

    /// Trichotomous Comparison (`cmp.t` / `<=>` operator of REBEL-6 paper):
    /// Returns `Positive (+1)` if `a > b`, `Zero (0)` if `a == b`, `Negative (-1)` if `a < b`.
    #[inline(always)]
    pub const fn cmp_t(a: i64, b: i64) -> Self {
        if a > b {
            Self::Positive
        } else if a < b {
            Self::Negative
        } else {
            Self::Zero
        }
    }

    /// Structural 1-trit ternary full adder:
    /// Composed of two cascaded half-adders (`half_add`) and a carry `sum_mod3` gate,
    /// without host CPU binary addition.
    /// Returns `(sum, carry_out)` for `self + other + carry_in`.
    #[inline(always)]
    pub const fn full_add(self, other: Self, carry_in: Self) -> (Self, Self) {
        let (s1, c1) = self.half_add(other);
        let (sum, c2) = s1.half_add(carry_in);
        let carry_out = c1.sum_mod3(c2);
        (sum, carry_out)
    }

    /// 2-bit dual-bit encoding for compact storage:
    /// `0b00` -> Zero (0), `0b01` -> Positive (+1), `0b10` -> Negative (-1).
    #[inline(always)]
    pub const fn to_dual_bits(self) -> u8 {
        match self {
            Self::Zero => 0b00,
            Self::Positive => 0b01,
            Self::Negative => 0b10,
        }
    }

    /// Decodes a dual-bit pair (`u8` masked with `0b11`) into a `Trit`.
    #[inline(always)]
    #[must_use]
    pub const fn from_dual_bits(bits: u8) -> Self {
        match bits & 0b11 {
            0b01 => Self::Positive,
            0b10 => Self::Negative,
            _ => Self::Zero,
        }
    }

    /// Canonical balanced ternary notation character (`'-'`, `'0'`, `'+'`).
    #[inline(always)]
    pub const fn to_char(self) -> char {
        match self {
            Self::Negative => '-',
            Self::Zero => '0',
            Self::Positive => '+',
        }
    }

    /// Converts character (`'-'`/`'T'`, `'0'`, `'+'`/`'1'`) to `Option<Trit>`.
    #[inline(always)]
    pub const fn from_char(c: char) -> Option<Self> {
        match c {
            '-' | 'T' | 't' => Some(Self::Negative),
            '0' => Some(Self::Zero),
            '+' | '1' => Some(Self::Positive),
            _ => None,
        }
    }
}

impl Neg for Trit {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self::Output {
        self.invert()
    }
}

impl Not for Trit {
    type Output = Self;

    #[inline(always)]
    fn not(self) -> Self::Output {
        self.invert()
    }
}

impl Mul for Trit {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self::Output {
        self.t_mul(rhs)
    }
}

impl BitAnd for Trit {
    type Output = Self;

    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self::Output {
        self.t_and(rhs)
    }
}

impl BitOr for Trit {
    type Output = Self;

    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self::Output {
        self.t_or(rhs)
    }
}

impl BitXor for Trit {
    type Output = Self;

    #[inline(always)]
    fn bitxor(self, rhs: Self) -> Self::Output {
        self.t_mul(rhs)
    }
}

impl fmt::Display for Trit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trit_inversion_and_symmetry() {
        assert_eq!(-Trit::Positive, Trit::Negative);
        assert_eq!(-Trit::Negative, Trit::Positive);
        assert_eq!(-Trit::Zero, Trit::Zero);
        assert_eq!(Trit::Positive.as_i8() + Trit::Negative.as_i8(), 0);
    }

    #[test]
    fn test_trit_full_adder() {
        // 1 + 1 + 0 = -1 with carry +1 (since 2 = 1*3 - 1)
        assert_eq!(
            Trit::Positive.full_add(Trit::Positive, Trit::Zero),
            (Trit::Negative, Trit::Positive)
        );
        // -1 + -1 + -1 = 0 with carry -1 (since -3 = -1*3 + 0)
        assert_eq!(
            Trit::Negative.full_add(Trit::Negative, Trit::Negative),
            (Trit::Zero, Trit::Negative)
        );
        // 1 + -1 + 0 = 0 with carry 0
        assert_eq!(
            Trit::Positive.full_add(Trit::Negative, Trit::Zero),
            (Trit::Zero, Trit::Zero)
        );
    }

    #[test]
    fn test_trit_dual_bit_roundtrip() {
        for t in [Trit::Negative, Trit::Zero, Trit::Positive] {
            let bits = t.to_dual_bits();
            assert_eq!(Trit::from_dual_bits(bits), t);
        }
    }
}
