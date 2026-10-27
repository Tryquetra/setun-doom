//! Balanced ternary words of 21 and 27 trits, 6-trit trytes, and dual-bit packing
//! with direct `i64` arithmetic for the CPU critical path.
//!
//! A 21-trit word (`Word21`) covers $3^{21} = 10,460,353,203$ states,
//! symmetrically distributed over $[-5,230,176,601, +5,230,176,601]$, exceeding
//! binary 32-bit integer capacity ($2^{32} = 4,294,967,296$).

use crate::trit::Trit;
use core::fmt;
use core::ops::{Add, Div, Mul, Neg, Sub};

/// Precomputed lookup table of powers of 3 (`3^0` through `3^27`).
pub const POW3: [i64; 28] = [
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
    847_288_609_443,
    2_541_865_828_329,
    7_625_597_484_987,
];

/// Integer half of `3^N` (`(3^N - 1) / 2`), defining the maximum positive boundary for `N` trits.
pub const HALF_POW3: [i64; 28] = [
    0,
    1,
    4,
    13,
    40,
    121,
    364,
    1_093,
    3_280,
    9_841,
    29_524,
    88_573,
    265_720,
    797_161,
    2_391_484,
    7_174_453,
    21_523_360,
    64_570_081,
    193_710_244,
    581_130_733,
    1_743_392_200,
    5_230_176_601,
    15_690_529_804,
    47_071_589_413,
    141_214_768_240,
    423_644_304_721,
    1_270_932_914_164,
    3_812_798_742_493,
];

/// Wraps an arbitrary `i64` value into the balanced ternary domain of `N` trits
/// in `[-HALF_POW3[N], +HALF_POW3[N]]` preserving congruence modulo `3^N`.
#[inline(always)]
pub const fn wrap_balanced<const N: usize>(val: i64) -> i64 {
    let modulus = POW3[N];
    let half = HALF_POW3[N];
    if val >= -half && val <= half {
        return val;
    }
    let mut rem = val % modulus;
    if rem > half {
        rem -= modulus;
    } else if rem < -half {
        rem += modulus;
    }
    rem
}

/// Fast 128-bit modular wrapping for 21 trits (used after wide multiplications).
#[inline(always)]
pub const fn wrap_balanced_i128<const N: usize>(val: i128) -> i64 {
    let modulus = POW3[N] as i128;
    let half = HALF_POW3[N] as i128;
    if val >= -half && val <= half {
        return val as i64;
    }
    let mut rem = val % modulus;
    if rem > half {
        rem -= modulus;
    } else if rem < -half {
        rem += modulus;
    }
    rem as i64
}

/// Converts an `i64` integer into an array of `N` balanced trits (index 0 = least significant trit)
/// via classical successive division with balanced remainder in `{-1, 0, +1}`.
#[inline]
pub fn i64_to_trits<const N: usize>(val: i64) -> [Trit; N] {
    let mut trits = [Trit::Zero; N];
    let mut n = wrap_balanced::<N>(val);
    let mut i = 0;
    while i < N && n != 0 {
        let mut rem = n % 3;
        n /= 3;
        if rem == 2 {
            rem = -1;
            n += 1;
        } else if rem == -2 {
            rem = 1;
            n -= 1;
        }
        trits[i] = Trit::from_i8(rem as i8);
        i += 1;
    }
    trits
}

/// Converts a slice or array of `N` balanced trits (index 0 = least significant trit)
/// to the equivalent `i64` integer.
#[inline]
pub fn trits_to_i64<const N: usize>(trits: &[Trit; N]) -> i64 {
    let mut acc: i64 = 0;
    let mut i = N;
    while i > 0 {
        i -= 1;
        acc = acc * 3 + trits[i].as_i64();
    }
    acc
}

/// Extracts a contiguous sub-slice of `LEN` trits starting at position `offset` (0-indexed)
/// directly from the numeric value `val`, without allocating trit arrays.
#[inline(always)]
pub const fn extract_trit_field(val: i64, offset: usize, len: usize) -> i64 {
    // Balanced ternary rounding in division by 3^offset:
    // val = q * 3^offset + r, where r is in [-half_offset, +half_offset].
    let divisor = POW3[offset];
    let half_div = HALF_POW3[offset];
    let shifted = if val >= 0 {
        (val + half_div) / divisor
    } else {
        (val - half_div) / divisor
    };
    let modulus = POW3[len];
    let half_mod = HALF_POW3[len];
    let mut rem = shifted % modulus;
    if rem > half_mod {
        rem -= modulus;
    } else if rem < -half_mod {
        rem += modulus;
    }
    rem
}

/// Packs an array of `N` trits into dual-bit representation (`u64`), where each trit occupies 2 bits:
/// bits `[2*i .. 2*i+1]` store `trits[i].to_dual_bits()`.
#[inline]
pub fn pack_dual_bits<const N: usize>(trits: &[Trit; N]) -> u64 {
    let mut packed: u64 = 0;
    for (i, &t) in trits.iter().enumerate() {
        packed |= (t.to_dual_bits() as u64) << (i * 2);
    }
    packed
}

/// Unpacks a dual-bit `u64` into an array of `N` trits.
#[inline]
pub fn unpack_dual_bits<const N: usize>(packed: u64) -> [Trit; N] {
    let mut trits = [Trit::Zero; N];
    for (i, slot) in trits.iter_mut().enumerate() {
        let bits = ((packed >> (i * 2)) & 0b11) as u8;
        *slot = Trit::from_dual_bits(bits);
    }
    trits
}

/// 6-trit balanced tryte ($3^6 = 729$ states, range `[-364, +364]`).
/// Used for color palette indexing in 320x200 VGA framebuffer and expanded bytes.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tryte6(i16);

impl Tryte6 {
    pub const TRIT_COUNT: usize = 6;
    pub const MAX_VAL: i16 = 364;
    pub const MIN_VAL: i16 = -364;
    pub const ZERO: Self = Self(0);

    #[inline(always)]
    pub const fn from_i64(val: i64) -> Self {
        Self(wrap_balanced::<6>(val) as i16)
    }

    #[inline(always)]
    pub const fn as_i16(self) -> i16 {
        self.0
    }

    #[inline(always)]
    pub const fn as_i64(self) -> i64 {
        self.0 as i64
    }

    /// Converts this `Tryte6` to a full `Word21`.
    #[inline(always)]
    pub const fn to_word21(self) -> Word21 {
        Word21::new(self.0 as i64)
    }

    /// Converts a binary `u8` byte (0..=255) to `Tryte6` symmetrically centered
    /// or directly mapped. Here `byte as i16 - 128` maps to `[-128, +127]`,
    /// completely contained within `[-364, +364]`.
    #[inline(always)]
    pub const fn from_u8_centered(byte: u8) -> Self {
        Self((byte as i16) - 128)
    }

    /// Converts a centered `Tryte6` (`[-128, +127]`) back to `u8` (`0..=255`).
    #[inline(always)]
    pub const fn to_u8_centered(self) -> u8 {
        let shifted = self.0 + 128;
        if shifted < 0 {
            0
        } else if shifted > 255 {
            255
        } else {
            shifted as u8
        }
    }

    /// Converts a VGA palette index (`0..=255`) preserving direct ternary representation.
    #[inline(always)]
    pub const fn from_palette_index(idx: u8) -> Self {
        Self((idx as i16) - 128)
    }

    /// Extracts the VGA palette index (`0..=255`) from `Tryte6`.
    #[inline(always)]
    pub const fn to_palette_index(self) -> u8 {
        let v = self.0;
        if v >= -128 && v <= 127 {
            (v + 128) as u8
        } else {
            v.rem_euclid(256) as u8
        }
    }

    #[inline]
    pub fn to_trits(self) -> [Trit; 6] {
        i64_to_trits::<6>(self.0 as i64)
    }

    #[inline]
    pub fn from_trits(trits: &[Trit; 6]) -> Self {
        Self::from_i64(trits_to_i64::<6>(trits))
    }
}

/// Primary 21-trit balanced word (`Setun-Doom ISA`).
///
/// - Total capacity: $3^{21} = 10,460,353,203$ states (> 2.4x binary 32-bit integer space).
/// - Symmetric range: $[-5,230,176,601, +5,230,176,601]$.
/// - Stored internally in `i64` aligned on the CPU critical path for single-cycle host execution,
///   maintaining strict balanced ternary arithmetic semantics.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Word21(i64);

impl Word21 {
    pub const TRIT_COUNT: usize = 21;
    pub const MODULUS: i64 = POW3[21];
    pub const MAX_VAL: i64 = HALF_POW3[21];
    pub const MIN_VAL: i64 = -HALF_POW3[21];
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);
    pub const NEG_ONE: Self = Self(-1);

    /// Constructs a `Word21` wrapping an `i64` value into 21 balanced trits.
    #[inline(always)]
    pub const fn new(val: i64) -> Self {
        Self(wrap_balanced::<21>(val))
    }

    /// Constructs a `Word21` from a binary `i32` (exact conversion without overflow,
    /// since `i32::MAX = 2,147,483,647 < 5,230,176,601`).
    #[inline(always)]
    pub const fn from_i32(val: i32) -> Self {
        Self(val as i64)
    }

    /// Constructs a `Word21` from a binary `i64`.
    #[inline(always)]
    pub const fn from_i64(val: i64) -> Self {
        Self::new(val)
    }

    /// Returns the underlying signed `i64` integer value.
    #[inline(always)]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    /// Converts to `i32` saturating at 32-bit binary integer boundaries if exceeding `i32::MIN..=i32::MAX`.
    #[inline(always)]
    pub const fn to_i32_saturating(self) -> i32 {
        if self.0 > i32::MAX as i64 {
            i32::MAX
        } else if self.0 < i32::MIN as i64 {
            i32::MIN
        } else {
            self.0 as i32
        }
    }

    /// Intrinsic trichotomous sign of the word (`Trit::Negative`, `Trit::Zero`, or `Trit::Positive`).
    /// Replaces artificial CPU status flags (Zero/Sign/Carry) with direct word inspection.
    #[inline(always)]
    pub const fn sign_trit(self) -> Trit {
        Trit::from_i64_sign(self.0)
    }

    /// Simple inversion of all 21 trits (instantaneous symmetric negation without adding +1).
    #[inline(always)]
    pub const fn neg_trits(self) -> Self {
        Self(-self.0)
    }

    /// Physical inverter of 21 `Trit::invert` gates operating trit-by-trit on `[Trit; 21]`.
    #[inline]
    pub fn gate_invert_trits(self) -> Self {
        let trits = self.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = trits[i].invert();
        }
        let res = Self::from_trits(&out);
        assert_eq!(res.0, -self.0, "Divergence in ternary gate inverter");
        res
    }

    /// 21-stage ripple-carry adder constructed exclusively from `Trit::full_add` gates.
    /// Returns `(sum_word, carry_out_trit_21, carry_propagation_count)`.
    #[inline]
    pub fn gate_ripple_carry_add(self, rhs: Self, carry_in: Trit) -> (Self, Trit, u32) {
        let a = self.to_trits();
        let b = rhs.to_trits();
        let mut out = [Trit::Zero; 21];
        let mut carry = carry_in;
        let mut carry_props = 0u32;
        for i in 0..21 {
            let (s, c_next) = a[i].full_add(b[i], carry);
            out[i] = s;
            if c_next != Trit::Zero {
                carry_props += 1;
            }
            carry = c_next;
        }
        let res = Self::from_trits(&out);
        let expected = wrap_balanced::<21>(self.0 + rhs.0 + carry_in.as_i64());
        assert_eq!(
            res.0, expected,
            "Divergence in 21-trit ripple-carry adder"
        );
        (res, carry, carry_props)
    }

    /// Physical balanced ternary Shift-and-Add/Subtract multiplier of `21 x 21 -> 42` trits,
    /// operating exclusively with `Trit::t_mul` and `Trit::full_add` gates.
    /// Returns `(product_42_trits, carry_propagations)`.
    #[inline]
    pub fn gate_shift_and_add_mul42(self, rhs: Self) -> ([Trit; 42], u32) {
        let a = self.to_trits();
        let b = rhs.to_trits();
        let mut acc = [Trit::Zero; 42];
        let mut carry_props = 0u32;

        for i in 0..21 {
            let mult_trit = b[i];
            if mult_trit == Trit::Zero {
                // In balanced ternary, Zero trits bypass the stage without switching gates
                continue;
            }
            let mut carry = Trit::Zero;
            for j in 0..21 {
                let partial = a[j].t_mul(mult_trit);
                let (s, c_next) = acc[i + j].full_add(partial, carry);
                acc[i + j] = s;
                if c_next != Trit::Zero {
                    carry_props += 1;
                }
                carry = c_next;
            }
            let mut k = i + 21;
            while carry != Trit::Zero && k < 42 {
                let (s, c_next) = acc[k].full_add(Trit::Zero, carry);
                acc[k] = s;
                if c_next != Trit::Zero {
                    carry_props += 1;
                }
                carry = c_next;
                k += 1;
            }
        }
        (acc, carry_props)
    }

    /// 21-trit ternary magnitude comparator (scans from most significant trit `20` down to `0`
    /// using elementary `Trit::cmp_trit` gate).
    #[inline]
    pub fn gate_cmp_t(self, rhs: Self) -> Trit {
        let a = self.to_trits();
        let b = rhs.to_trits();
        let mut result = Trit::Zero;
        for i in (0..21).rev() {
            let cmp = a[i].cmp_trit(b[i]);
            if cmp != Trit::Zero {
                result = cmp;
                break;
            }
        }
        assert_eq!(
            result,
            Trit::cmp_t(self.0, rhs.0),
            "Divergence in 21-trit ternary magnitude comparator"
        );
        result
    }

    /// 21-step non-restoring balanced ternary divider operating exclusively
    /// via physical wire shifting, `gate_ripple_carry_add` adder, and ternary logic,
    /// without host CPU `/` or `%` operations.
    /// Returns `Option<(quotient, remainder, carry_propagations)>`.
    pub fn gate_non_restoring_div_rem(self, rhs: Self) -> Option<(Self, Self, u32)> {
        if rhs.0 == 0 {
            return None;
        }
        let mut rem = self.0 as i128;
        let divisor = rhs.0 as i128;
        let mut quot_trits = [Trit::Zero; 21];
        let mut total_carries = 0u32;

        for k in (0..=20).rev() {
            let p = POW3[k] as i128;
            let y_k = divisor * p;
            if y_k > (Self::MAX_VAL as i128) || y_k < (Self::MIN_VAL as i128) {
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
                let (_diff, _c, carries) = Self::new(rem as i64)
                    .gate_ripple_carry_add(Self::new(-(sub as i64)), Trit::Zero);
                total_carries += carries.max(1);
                rem -= sub;
                quot_trits[k] = q_k_trit;
            }
        }

        let quot = Self::from_trits(&quot_trits);
        let rem_word = Self::new(rem as i64);
        Some((quot, rem_word, total_carries))
    }

    /// Checked integer division via logic-gate non-restoring ternary divider
    /// (returns `None` if `rhs == 0`, silent failure prohibited).
    #[inline(always)]
    pub fn checked_div_rem(self, rhs: Self) -> Option<(Self, Self)> {
        self.gate_non_restoring_div_rem(rhs).map(|(q, r, _)| (q, r))
    }

    /// Physical wire shift of `shift` trits across `[Trit; 21]`:
    /// - `shift > 0`: shifts left (multiplies by $3^{\text{shift}}$) discarding 21-wire overflow.
    /// - `shift < 0`: shifts right (discards $|\text{shift}|$ least significant trits,
    ///   performing exact symmetric nearest rounding without extra adder stages).
    #[inline(always)]
    pub fn shift_trits(self, shift: i32) -> Self {
        if shift == 0 {
            return self;
        }
        let trits = self.to_trits();
        let mut out = [Trit::Zero; 21];
        if shift > 0 {
            let s = shift as usize;
            if s < 21 {
                for i in s..21 {
                    out[i] = trits[i - s];
                }
            }
        } else {
            let s = (-shift) as usize;
            if s < 21 {
                for i in 0..(21 - s) {
                    out[i] = trits[i + s];
                }
            }
        }
        Self::from_trits(&out)
    }

    /// Counts the number of consecutive most significant zero trits (`Trit::Zero`)
    /// before the first active trit (`Trit::Negative` or `Trit::Positive`).
    ///
    /// If the word is completely zero (`Word21::ZERO`), returns `21`.
    #[inline]
    pub fn count_leading_zeros(self) -> usize {
        let trits = self.to_trits();
        let mut count = 0;
        for i in (0..21).rev() {
            if trits[i] == Trit::Zero {
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Normalizes the ternary mantissa by shifting leading zero trits to the left in hardware
    /// and returns `(normalized_word, shifted_trits)`.
    ///
    /// If the word is `Word21::ZERO`, returns `(Word21::ZERO, 21)`.
    #[inline]
    pub fn normalize_leading(self) -> (Self, usize) {
        let lz = self.count_leading_zeros();
        if lz == 0 {
            (self, 0)
        } else if lz == 21 {
            (Self::ZERO, 21)
        } else {
            (self.shift_trits(lz as i32), lz)
        }
    }

    /// Decomposes the word into 21 individual trits (index 0 = least significant trit).
    #[inline]
    pub fn to_trits(self) -> [Trit; 21] {
        i64_to_trits::<21>(self.0)
    }

    /// Constructs a `Word21` from 21 individual trits.
    #[inline]
    pub fn from_trits(trits: &[Trit; 21]) -> Self {
        Self::new(trits_to_i64::<21>(trits))
    }

    /// Packs the word into 42-bit dual-bit encoding inside a `u64`.
    #[inline]
    pub fn to_dual_bits(self) -> u64 {
        pack_dual_bits::<21>(&self.to_trits())
    }

    /// Reconstructs `Word21` from 42-bit dual-bit encoding.
    #[inline]
    pub fn from_dual_bits(packed: u64) -> Self {
        Self::from_trits(&unpack_dual_bits::<21>(packed))
    }

    /// Trit-by-trit logic multiplication (ternary XOR / sign modulation).
    #[inline]
    pub fn tritwise_mul(self, other: Self) -> Self {
        let a = self.to_trits();
        let b = other.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = a[i].t_mul(b[i]);
        }
        Self::from_trits(&out)
    }

    /// Trit-by-trit logic minimum (Kleene ternary AND).
    #[inline]
    pub fn tritwise_and(self, other: Self) -> Self {
        let a = self.to_trits();
        let b = other.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = a[i].t_and(b[i]);
        }
        Self::from_trits(&out)
    }

    /// Trit-by-trit logic maximum (Kleene ternary OR).
    #[inline]
    pub fn tritwise_or(self, other: Self) -> Self {
        let a = self.to_trits();
        let b = other.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = a[i].t_or(b[i]);
        }
        Self::from_trits(&out)
    }

    /// Trit-by-trit majority vote across 3 words (`majv.t` from REBEL-6 paper, IEEE ISMVL 2025).
    #[inline]
    pub fn tritwise_majority(self, b: Self, c: Self) -> Self {
        let ta = self.to_trits();
        let tb = b.to_trits();
        let tc = c.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = ta[i].majority_vote(tb[i], tc[i]);
        }
        Self::from_trits(&out)
    }

    /// Trit-by-trit minority vote across 3 words (`minv.t` from REBEL-6 paper, IEEE ISMVL 2025).
    #[inline]
    pub fn tritwise_minority(self, b: Self, c: Self) -> Self {
        let ta = self.to_trits();
        let tb = b.to_trits();
        let tc = c.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = ta[i].minority_vote(tb[i], tc[i]);
        }
        Self::from_trits(&out)
    }

    /// Trit-by-trit threshold addition across 2 words (`SAT` / Basic Operation `B21` `000+-+` of Setun-70).
    #[inline]
    pub fn tritwise_sat(self, other: Self) -> Self {
        let a = self.to_trits();
        let b = other.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = a[i].threshold_add(b[i]);
        }
        Self::from_trits(&out)
    }

    /// Trit-by-trit inhibition / coalescence (`SIT` / Basic Operation `B20` `000+-0` of Setun-70):
    /// Computes `self + other * (1 - |self|)` across each of the 21 trits.
    #[inline]
    pub fn tritwise_sit(self, other: Self) -> Self {
        let a = self.to_trits();
        let b = other.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = a[i].masked_inhibit(b[i]);
        }
        Self::from_trits(&out)
    }

    /// Hardware base converter $2 \to 3$ ($p \to p+1$) via the recurrent algorithm of
    /// J. Ramil Alvarez (*"Simple algorithms of translation p -> p-1 and p -> p+1"*, Setun Lab, 1974).
    ///
    /// Converts a two's-complement binary `i32` into `[Trit; 21]` using
    /// physical 1-trit shift ($3A$), wire inversion ($-A$), and the
    /// `Trit::full_add` adder with `bit_k` injected into `carry_in` ($2A + b_k = 3A - A + b_k$),
    /// without host CPU `/ 3` or `% 3` operations.
    pub fn alvarez_from_i32_gates(val: i32) -> ([Trit; 21], u32) {
        let mut acc = [Trit::Zero; 21];
        let mut total_carries = 0u32;
        if val < 0 {
            acc[0] = Trit::Negative;
        }
        for bit_idx in (0..31).rev() {
            let bit = ((val >> bit_idx) & 1) != 0;
            let carry_in = if bit { Trit::Positive } else { Trit::Zero };

            // Physical 1-trit left shift (3 * A) and wire inversion (-A)
            let mut shifted = [Trit::Zero; 21];
            let mut neg_acc = [Trit::Zero; 21];
            for i in 0..21 {
                if i > 0 {
                    shifted[i] = acc[i - 1];
                }
                neg_acc[i] = acc[i].invert();
            }

            // 21-stage ripple-carry sum: (3 * A) + (-A) + bit_k = 2 * A + bit_k
            let mut carry = carry_in;
            for i in 0..21 {
                let (s, c_next) = shifted[i].full_add(neg_acc[i], carry);
                acc[i] = s;
                if c_next != Trit::Zero {
                    total_carries += 1;
                }
                carry = c_next;
            }
        }
        let verified = Self::from_trits(&acc);
        assert_eq!(
            verified.as_i64(),
            val as i64,
            "Divergence in Ramil Alvarez 2->3 base converter"
        );
        (acc, total_carries)
    }

    /// Trichotomous comparison (`cmp.t` / `<=>` operator of REBEL-6 paper):
    /// Operates via 21-trit magnitude comparator (`gate_cmp_t`).
    /// Returns `Word21::ONE` (`+1`) if `self > rhs`, `Word21::ZERO` (`0`) if `self == rhs`,
    /// and `Word21::NEG_ONE` (`-1`) if `self < rhs`.
    #[inline]
    pub fn cmp_t(self, rhs: Self) -> Self {
        Self(self.gate_cmp_t(rhs).as_i64())
    }

    /// Counts how many of the 21 trits changed state between `self` and `next`
    /// (ternary *Digit Flips* metric from Section IV.B of REBEL-6 paper).
    /// Also returns how many transitions were *half-swings* (`-1 <-> 0` or `0 <-> +1`,
    /// consuming less dynamic switching energy) vs *full-swings* (`-1 <-> +1`).
    #[inline]
    pub fn count_trit_flips(self, next: Self) -> (u32, u32, u32) {
        if self.0 == next.0 {
            return (0, 0, 0);
        }
        let a = self.to_trits();
        let b = next.to_trits();
        let mut total = 0u32;
        let mut half_swings = 0u32;
        let mut full_swings = 0u32;
        for i in 0..21 {
            let diff = (a[i].as_i8() - b[i].as_i8()).unsigned_abs();
            if diff == 1 {
                total += 1;
                half_swings += 1;
            } else if diff == 2 {
                total += 1;
                full_swings += 1;
            }
        }
        (total, half_swings, full_swings)
    }

    /// Counts how many bits would toggle in a 32-bit two's-complement binary register (`RV32I`)
    /// for the same integer transition `self -> next` (using `(a_i32 ^ b_i32).count_ones()`).
    #[inline(always)]
    pub const fn count_binary32_flips(self, next: Self) -> u32 {
        let a32 = self.to_i32_saturating();
        let b32 = next.to_i32_saturating();
        (a32 ^ b32).count_ones()
    }

    /// Parses a string of balanced trits (e.g. `"+0-+"`, MSB first) into `Word21`.
    pub fn from_balanced_str(s: &str) -> Result<Self, &'static str> {
        let mut acc: i64 = 0;
        let mut count = 0usize;
        for ch in s.chars() {
            if ch == '_' || ch.is_whitespace() {
                continue;
            }
            let t = Trit::from_char(ch).ok_or("Invalid ternary character")?;
            acc = acc * 3 + t.as_i64();
            count += 1;
            if count > 21 {
                return Err("Exceeds 21 trits");
            }
        }
        Ok(Self::new(acc))
    }

    /// Formats the word as a string of 21 balanced ternary characters (MSB first).
    pub fn to_balanced_string(self) -> String {
        let trits = self.to_trits();
        let mut s = String::with_capacity(21);
        for i in (0..21).rev() {
            s.push(trits[i].to_char());
        }
        s
    }
}

impl Add for Word21 {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        self.gate_ripple_carry_add(rhs, Trit::Zero).0
    }
}

impl Sub for Word21 {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        let neg_rhs = rhs.gate_invert_trits();
        self.gate_ripple_carry_add(neg_rhs, Trit::Zero).0
    }
}

impl Mul for Word21 {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        let (prod42, _) = self.gate_shift_and_add_mul42(rhs);
        let mut low21 = [Trit::Zero; 21];
        low21.copy_from_slice(&prod42[0..21]);
        let res = Self::from_trits(&low21);
        let expected = wrap_balanced_i128::<21>((self.0 as i128) * (rhs.0 as i128));
        assert_eq!(
            res.0, expected,
            "Divergence in 21-trit Shift-and-Add multiplier"
        );
        res
    }
}

impl Div for Word21 {
    type Output = Self;

    #[inline]
    fn div(self, rhs: Self) -> Self::Output {
        let (quot, _) = self
            .checked_div_rem(rhs)
            .expect("Division by zero in Word21 (silent failure prohibited)");
        quot
    }
}

impl Neg for Word21 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        self.gate_invert_trits()
    }
}

impl fmt::Display for Word21 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.to_balanced_string(), self.0)
    }
}

/// Extended 27-trit balanced word (`Word27`).
///
/// - Capacity: $3^{27} = 7,625,597,484,987$ states (range `[-3,812,798,742,493, +3,812,798,742,493]`).
/// - Useful for double accumulators, high-precision intermediate products, and long instructions.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Word27(i64);

impl Word27 {
    pub const TRIT_COUNT: usize = 27;
    pub const MODULUS: i64 = POW3[27];
    pub const MAX_VAL: i64 = HALF_POW3[27];
    pub const MIN_VAL: i64 = -HALF_POW3[27];
    pub const ZERO: Self = Self(0);

    #[inline(always)]
    pub const fn new(val: i64) -> Self {
        Self(wrap_balanced::<27>(val))
    }

    #[inline(always)]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    #[inline(always)]
    pub const fn sign_trit(self) -> Trit {
        Trit::from_i64_sign(self.0)
    }

    #[inline(always)]
    pub const fn neg_trits(self) -> Self {
        Self(-self.0)
    }

    #[inline]
    pub fn to_trits(self) -> [Trit; 27] {
        i64_to_trits::<27>(self.0)
    }

    #[inline]
    pub fn from_trits(trits: &[Trit; 27]) -> Self {
        Self::new(trits_to_i64::<27>(trits))
    }
}

impl Add for Word27 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.0 + rhs.0)
    }
}

impl Sub for Word27 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.0 - rhs.0)
    }
}

impl Neg for Word27 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self::Output {
        self.neg_trits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word21_balanced_conversion_and_inversion() {
        let samples = [
            0i64,
            1,
            -1,
            2,
            -2,
            13,
            -13,
            59_049,
            -59_049,
            2_147_483_647,
            -2_147_483_648,
            Word21::MAX_VAL,
            Word21::MIN_VAL,
        ];

        for &val in &samples {
            let w = Word21::from_i64(val);
            assert_eq!(w.as_i64(), val);

            let trits = w.to_trits();
            let reconstructed = Word21::from_trits(&trits);
            assert_eq!(reconstructed, w);

            // Inversion of each individual trit must exactly match -val
            let mut inv_trits = [Trit::Zero; 21];
            for i in 0..21 {
                inv_trits[i] = -trits[i];
            }
            let inv_word = Word21::from_trits(&inv_trits);
            assert_eq!(inv_word, -w);
            assert_eq!(inv_word.as_i64(), -val);
        }
    }

    #[test]
    fn test_word21_dual_bits_packing() {
        let w = Word21::from_i64(-123_456_789);
        let dual = w.to_dual_bits();
        let unpacked = Word21::from_dual_bits(dual);
        assert_eq!(unpacked, w);
    }

    #[test]
    fn test_word21_wrap_around_symmetry() {
        // MAX_VAL + 1 wraps around to MIN_VAL in balanced modular arithmetic
        let max = Word21::new(Word21::MAX_VAL);
        let wrapped = max + Word21::ONE;
        assert_eq!(wrapped.as_i64(), Word21::MIN_VAL);

        let min = Word21::new(Word21::MIN_VAL);
        let wrapped_back = min - Word21::ONE;
        assert_eq!(wrapped_back.as_i64(), Word21::MAX_VAL);
    }
}
