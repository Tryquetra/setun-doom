//! 24-trit balanced ternary word (`Word24`) for the REBEL-6 core.
//!
//! Range: `[-141_214_768_240, +141_214_768_240]`, the unique balanced representation of
//! `3^24 = 282_429_536_481` states. Arithmetic wraps modulo `3^24`, matching the REBEL-6
//! specification. Binary instructions reinterpret the same word as an `i32` and wrap modulo
//! `2^32`, which is the Base Binary layer.

/// Number of trits in a REBEL-6 register.
pub const TRITS: usize = 24;

/// `3^24`.
pub const MODULUS: i64 = 282_429_536_481;

/// `(3^24 - 1) / 2`, the largest positive value.
pub const HALF: i64 = 141_214_768_240;

/// Powers of three from `3^0` to `3^24`.
pub const POW3: [i64; TRITS + 1] = [
    1,
    3,
    9,
    27,
    81,
    243,
    729,
    2_187,
    6_561,
    19_683,
    59_049,
    177_147,
    531_441,
    1_594_323,
    4_782_969,
    14_348_907,
    43_046_721,
    129_140_163,
    387_420_489,
    1_162_261_467,
    3_486_784_401,
    10_460_353_203,
    31_381_059_609,
    94_143_178_827,
    282_429_536_481,
];

/// Splits the least significant balanced trit from `value`, returning `(trit, rest)`.
#[must_use]
pub fn split_trit(value: i64) -> (i8, i64) {
    let mut digit = value % 3;
    let mut rest = value / 3;
    if digit == 2 {
        digit = -1;
        rest += 1;
    } else if digit == -2 {
        digit = 1;
        rest -= 1;
    }
    (digit as i8, rest)
}

/// Balanced ternary word of 24 trits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word24(i64);

impl Word24 {
    /// Zero.
    pub const ZERO: Self = Self(0);
    /// One.
    pub const ONE: Self = Self(1);
    /// Negative one.
    pub const NEG_ONE: Self = Self(-1);
    /// Most negative value.
    pub const MIN: Self = Self(-HALF);
    /// Most positive value.
    pub const MAX: Self = Self(HALF);

    /// Wraps `value` into the balanced range modulo `3^24`.
    #[must_use]
    pub fn new(value: i64) -> Self {
        Self(Self::wrap(value))
    }

    /// Wraps an `i64` into `[-HALF, HALF]`.
    #[must_use]
    pub const fn wrap(value: i64) -> i64 {
        let mut r = value % MODULUS;
        if r > HALF {
            r -= MODULUS;
        } else if r < -HALF {
            r += MODULUS;
        }
        r
    }

    /// Builds a word from a raw value already inside the balanced range.
    ///
    /// # Panics
    ///
    /// Panics in debug builds when `value` is outside `[-HALF, HALF]`.
    #[must_use]
    pub const fn from_raw(value: i64) -> Self {
        debug_assert!(value >= -HALF && value <= HALF);
        Self(value)
    }

    /// Returns the raw `i64` value.
    #[must_use]
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// Returns trit `index` (`0` is the least significant trit).
    ///
    /// # Panics
    ///
    /// Panics when `index >= 24`.
    #[must_use]
    pub fn trit(self, index: usize) -> i8 {
        assert!(index < TRITS);
        let mut value = self.0;
        let mut digit = 0;
        for _ in 0..=index {
            let (next, rest) = split_trit(value);
            digit = next;
            value = rest;
        }
        digit
    }

    /// Builds a word from balanced trits, least significant first. Extra trits wrap.
    #[must_use]
    pub fn from_trits(trits: &[i8]) -> Self {
        let mut value = 0i64;
        for (i, &t) in trits.iter().take(TRITS).enumerate() {
            value += i64::from(t) * POW3[i];
        }
        Self::new(value)
    }

    /// Expands the word into 24 balanced trits, least significant first.
    #[must_use]
    pub fn to_trits(self) -> [i8; TRITS] {
        let mut out = [0i8; TRITS];
        let mut value = self.0;
        for slot in &mut out {
            let (digit, rest) = split_trit(value);
            *slot = digit;
            value = rest;
        }
        out
    }

    /// Returns the signed value of the lowest `width` trits.
    ///
    /// # Panics
    ///
    /// Panics when `width > 24`.
    #[must_use]
    pub fn low(self, width: usize) -> i64 {
        assert!(width <= TRITS);
        let mut value = 0i64;
        for i in 0..width {
            value += i64::from(self.trit(i)) * POW3[i];
        }
        value
    }

    /// Ternary addition modulo `3^24`.
    #[must_use]
    pub fn add(self, other: Self) -> Self {
        Self::new(self.0 + other.0)
    }

    /// Ternary subtraction modulo `3^24`.
    #[must_use]
    pub fn sub(self, other: Self) -> Self {
        Self::new(self.0 - other.0)
    }

    /// Ternary negation (`STI.T`), total on the balanced range.
    #[must_use]
    pub fn neg(self) -> Self {
        Self::new(-self.0)
    }

    /// Signed shift by `amount` trits. Positive shifts toward the most significant trit.
    /// Vacated positions receive `fill`.
    #[must_use]
    pub fn shift(self, amount: i32, fill: i8) -> Self {
        if amount == 0 {
            return self;
        }
        if amount.abs() >= TRITS as i32 {
            return Self::from_trits(&[fill; TRITS]);
        }
        let mut out = [0i8; TRITS];
        for (i, slot) in out.iter_mut().enumerate() {
            let src = i as i32 - amount;
            *slot = if (0..TRITS as i32).contains(&src) {
                self.trit(src as usize)
            } else {
                fill
            };
        }
        Self::from_trits(&out)
    }

    /// Cyclic rotation by `amount` trits, normalized modulo 24.
    #[must_use]
    pub fn rotate(self, amount: i32) -> Self {
        let k = amount.rem_euclid(TRITS as i32);
        let mut out = [0i8; TRITS];
        for (i, slot) in out.iter_mut().enumerate() {
            let src = (i as i32 - k).rem_euclid(TRITS as i32);
            *slot = self.trit(src as usize);
        }
        Self::from_trits(&out)
    }

    /// Three-way comparison: `+1` when `self > other`, `0` when equal, `-1` when smaller.
    #[must_use]
    pub fn cmp3(self, other: Self) -> i8 {
        if self.0 > other.0 {
            1
        } else if self.0 < other.0 {
            -1
        } else {
            0
        }
    }

    /// Wordwise minimum.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// Wordwise maximum.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    /// Tritwise minimum (`AND.T`).
    #[must_use]
    pub fn and_tritwise(self, other: Self) -> Self {
        Self::zip_trits(self, other, |a, b| a.min(b))
    }

    /// Tritwise maximum (`OR.T`).
    #[must_use]
    pub fn or_tritwise(self, other: Self) -> Self {
        Self::zip_trits(self, other, |a, b| a.max(b))
    }

    /// Tritwise `-(a * b)` (`XOR.T`).
    #[must_use]
    pub fn xor_tritwise(self, other: Self) -> Self {
        Self::zip_trits(self, other, |a, b| -(a * b))
    }

    /// Per-trit median of three words (`MAJV.T`).
    #[must_use]
    pub fn majority(self, b: Self, c: Self) -> Self {
        let mut out = [0i8; TRITS];
        for i in 0..TRITS {
            let mut lane = [self.trit(i), b.trit(i), c.trit(i)];
            lane.sort_unstable();
            out[i] = lane[1];
        }
        Self::from_trits(&out)
    }

    /// Per-trit negated median of three words (`MINV.T`).
    #[must_use]
    pub fn minority(self, b: Self, c: Self) -> Self {
        self.majority(b, c).neg()
    }

    /// Reinterprets the word as an `i32`, truncating modulo `2^32`.
    #[must_use]
    pub fn to_i32(self) -> i32 {
        self.0 as i32
    }

    /// Builds a word from an `i32`.
    #[must_use]
    pub fn from_i32(value: i32) -> Self {
        Self(i64::from(value))
    }

    /// Number of trits that differ between two words.
    #[must_use]
    pub fn trit_flips(self, other: Self) -> u32 {
        let mut flips = 0;
        for i in 0..TRITS {
            if self.trit(i) != other.trit(i) {
                flips += 1;
            }
        }
        flips
    }

    /// Weighted switching energy in quarter units: a half swing (`0 <-> ±1`) costs `1`,
    /// a full swing (`-1 <-> +1`) costs `4`.
    #[must_use]
    pub fn weighted_energy(self, other: Self) -> u32 {
        let mut energy = 0;
        for i in 0..TRITS {
            let a = self.trit(i);
            let b = other.trit(i);
            if a != b {
                energy += if a == 0 || b == 0 { 1 } else { 4 };
            }
        }
        energy
    }

    fn zip_trits(a: Self, b: Self, f: impl Fn(i8, i8) -> i8) -> Self {
        let mut out = [0i8; TRITS];
        for i in 0..TRITS {
            out[i] = f(a.trit(i), b.trit(i));
        }
        Self::from_trits(&out)
    }
}

impl core::fmt::Display for Word24 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_balanced_modulo_3_pow_24() {
        assert_eq!(Word24::new(HALF + 1).raw(), -HALF);
        assert_eq!(Word24::new(-HALF - 1).raw(), HALF);
        assert_eq!(Word24::new(MODULUS).raw(), 0);
        assert_eq!(Word24::new(-MODULUS).raw(), 0);
    }

    #[test]
    fn trit_round_trip() {
        for value in [-HALF, -1, 0, 1, HALF, 123_456_789] {
            let word = Word24::new(value);
            assert_eq!(Word24::from_trits(&word.to_trits()), word);
        }
    }

    #[test]
    fn addition_and_negation_are_exact() {
        let a = Word24::new(123_456_789);
        let b = Word24::new(-987_654_321);
        assert_eq!(a.add(b).raw(), 123_456_789 - 987_654_321);
        assert_eq!(a.add(a.neg()), Word24::ZERO);
        assert_eq!(Word24::MIN.neg(), Word24::MAX);
    }

    #[test]
    fn shift_direction_and_fill() {
        let one = Word24::ONE;
        assert_eq!(one.shift(3, 0).raw(), 27);
        assert_eq!(one.shift(-1, 0).raw(), 0);
        assert_eq!(Word24::ZERO.shift(2, 1).raw(), 4);
        assert_eq!(
            Word24::ZERO.shift(24, -1).raw(),
            Word24::from_trits(&[-1; TRITS]).raw()
        );
    }

    #[test]
    fn rotate_is_cyclic() {
        let one = Word24::ONE;
        assert_eq!(one.rotate(1).raw(), 3);
        assert_eq!(one.rotate(23).raw(), POW3[23]);
        assert_eq!(one.rotate(24), one);
        assert_eq!(one.rotate(-1), one.rotate(23));
    }

    #[test]
    fn tritwise_logic_tables() {
        let a = Word24::from_trits(&[-1, 0, 1, -1]);
        let b = Word24::from_trits(&[-1, 0, 1, 1]);
        assert_eq!(a.and_tritwise(b).to_trits()[..4], [-1, 0, 1, -1]);
        assert_eq!(a.or_tritwise(b).to_trits()[..4], [-1, 0, 1, 1]);
        assert_eq!(a.xor_tritwise(b).to_trits()[..4], [-1, 0, -1, 1]);
    }

    #[test]
    fn majority_is_median() {
        let a = Word24::from_trits(&[1, 1, 0, -1]);
        let b = Word24::from_trits(&[1, -1, 0, -1]);
        let c = Word24::from_trits(&[-1, 0, 0, 1]);
        assert_eq!(a.majority(b, c).to_trits()[..4], [1, 0, 0, -1]);
        assert_eq!(a.minority(b, c).to_trits()[..4], [-1, 0, 0, 1]);
    }

    #[test]
    fn flips_and_energy() {
        let zero = Word24::ZERO;
        let one = Word24::ONE;
        assert_eq!(zero.trit_flips(one), 1);
        assert_eq!(zero.weighted_energy(one), 1);
        assert_eq!(Word24::ONE.weighted_energy(Word24::NEG_ONE), 4);
    }

    #[test]
    fn binary_reinterpretation_truncates_modulo_2_pow_32() {
        assert_eq!(Word24::from_i32(-1).to_i32(), -1);
        assert_eq!(Word24::new(HALF).to_i32(), (HALF as i32));
        assert_eq!(Word24::new(4_294_967_297).to_i32(), 1);
    }
}
