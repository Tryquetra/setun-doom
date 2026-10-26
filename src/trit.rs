//! Representação e lógica ternária balanceada (Trits no conjunto {-1, 0, +1}).
//!
//! Inspirado na arquitetura eletromagnética do Setun (1958, Nikolai Brusentsov),
//! cada trit assume exatamente um de três estados simétricos em torno de zero.

use core::fmt;
use core::ops::{BitAnd, BitOr, BitXor, Mul, Neg, Not};

/// Estado elementar de um dígito ternário balanceado (Trit).
#[repr(i8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Trit {
    /// Estado negativo (-1), historicamente representado por `'T'` ou `'-'`.
    Negative = -1,
    /// Estado neutro (0), representado por `'0'`.
    #[default]
    Zero = 0,
    /// Estado positivo (+1), representado por `'+'` ou `'1'`.
    Positive = 1,
}

impl Trit {
    /// Converte um inteiro assinado (`i8`) para `Trit` saturando na faixa `[-1, +1]`.
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

    /// Converte um inteiro (`i64`) em `Trit` pelo sinal exato ou resto balanceado.
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

    /// Retorna o valor numérico como `i8` (-1, 0 ou 1).
    #[inline(always)]
    pub const fn as_i8(self) -> i8 {
        self as i8
    }

    /// Retorna o valor numérico como `i64` (-1, 0 ou 1).
    #[inline(always)]
    pub const fn as_i64(self) -> i64 {
        self as i64
    }

    /// Inversão ternária (negação simétrica): -1 <-> +1, 0 -> 0.
    #[inline(always)]
    pub const fn invert(self) -> Self {
        match self {
            Self::Negative => Self::Positive,
            Self::Zero => Self::Zero,
            Self::Positive => Self::Negative,
        }
    }

    /// Conjunção ternária (Kleene/Lukasiewicz MIN): porta lógica pura sem aritmética binária.
    #[inline(always)]
    pub const fn t_and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Negative, _) | (_, Self::Negative) => Self::Negative,
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (Self::Positive, Self::Positive) => Self::Positive,
        }
    }

    /// Disjunção ternária (Kleene/Lukasiewicz MAX): porta lógica pura sem aritmética binária.
    #[inline(always)]
    pub const fn t_or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Positive, _) | (_, Self::Positive) => Self::Positive,
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (Self::Negative, Self::Negative) => Self::Negative,
        }
    }

    /// Multiplicação ternária (isomorfa ao XOR ternário balanceado / modulador de sinal Setun):
    /// implementada por chaveamento de estados puro sem multiplicação binária.
    #[inline(always)]
    pub const fn t_mul(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, _) | (_, Self::Zero) => Self::Zero,
            (Self::Positive, x) => x,
            (Self::Negative, x) => x.invert(),
        }
    }

    /// Porta Consenso Ternária (`carry` de meio-somador):
    /// Retorna `Positive` se ambos forem `Positive`, `Negative` se ambos forem `Negative`,
    /// e `Zero` caso contrário.
    #[inline(always)]
    pub const fn consensus(self, other: Self) -> Self {
        match (self, other) {
            (Self::Positive, Self::Positive) => Self::Positive,
            (Self::Negative, Self::Negative) => Self::Negative,
            _ => Self::Zero,
        }
    }

    /// Porta Soma-Módulo-3 (`sum` de meio-somador ternário balanceado):
    /// Opera puramente por transições de estado no anel `{-1, 0, +1}`.
    #[inline(always)]
    pub const fn sum_mod3(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, x) | (x, Self::Zero) => x,
            (Self::Positive, Self::Positive) => Self::Negative,
            (Self::Negative, Self::Negative) => Self::Positive,
            (Self::Positive, Self::Negative) | (Self::Negative, Self::Positive) => Self::Zero,
        }
    }

    /// Meio-somador ternário balanceado (Half Adder de 1 trit) construído a partir das portas
    /// `sum_mod3` e `consensus`. Retorna `(soma, carry_out)`.
    #[inline(always)]
    pub const fn half_add(self, other: Self) -> (Self, Self) {
        (self.sum_mod3(other), self.consensus(other))
    }

    /// Voto Majoritário Ternário (`majv.t` do artigo REBEL-6, Tabela II):
    /// Se pelo menos 2 dos 3 trits `(self, b, c)` possuírem o mesmo estado `n`, retorna `n`;
    /// caso todos os 3 sejam distintos (`-1, 0, +1`), retorna `Trit::Zero`.
    #[inline(always)]
    pub const fn majority_vote(self, b: Self, c: Self) -> Self {
        match (self, b, c) {
            (x, y, _) if x as i8 == y as i8 => x,
            (x, _, z) if x as i8 == z as i8 => x,
            (_, y, z) if y as i8 == z as i8 => y,
            _ => Self::Zero,
        }
    }

    /// Voto Minoritário Ternário (`minv.t` do artigo REBEL-6, Tabela II):
    /// Quando dois trits são iguais e um é único, retorna o trit único (`cnt(n) == 1`).
    /// Se todos os três forem iguais ou todos os três forem distintos (`-1, 0, +1`), retorna `Trit::Zero`.
    #[inline(always)]
    pub const fn minority_vote(self, b: Self, c: Self) -> Self {
        match (self, b, c) {
            (x, y, z) if x as i8 == y as i8 && x as i8 != z as i8 => z,
            (x, y, z) if x as i8 == z as i8 && x as i8 != y as i8 => y,
            (x, y, z) if y as i8 == z as i8 && y as i8 != x as i8 => x,
            _ => Self::Zero,
        }
    }

    /// Adição Limiar de Trits (`SAT` / Operação Básica `B21` `000+-+` da Setun-70, Brusentsov 1970):
    /// Porta linear de limiar (*Threshold Addition*) entre dois trits `S` (`self`) e `T` (`other`):
    /// retorna `Positive (+1)` se `S + T > 0`, `Negative (-1)` se `S + T < 0` e `Zero (0)` se `S + T == 0`.
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

    /// Inibição / Coalescência Ternária (`SIT` / Operação Básica `B20` `000+-0` da Setun-70):
    /// Definida algebricamente por Brusentsov como `S + T * (1 - |S|)`:
    /// preserva `self` (`S`) se `self != Zero`; caso `self == Zero`, assume o estado de `other` (`T`).
    #[inline(always)]
    pub const fn masked_inhibit(self, other: Self) -> Self {
        match self {
            Self::Zero => other,
            non_zero => non_zero,
        }
    }

    /// Comparação Tricótoma de 1 trit (`cmp.t` elementar):
    /// Retorna `Positive (+1)` se `self > other`, `Zero (0)` se `self == other`, `Negative (-1)` se `self < other`.
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

    /// Comparação Tricótoma (`cmp.t` / operador `<=>` do artigo REBEL-6):
    /// Retorna `Positive (+1)` se `a > b`, `Zero (0)` se `a == b`, `Negative (-1)` se `a < b`.
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

    /// Somador completo ternário estrutural (Full Adder de 1 trit):
    /// Composto por dois meio-somadores (`half_add`) em cascata e uma porta `sum_mod3` de carry,
    /// sem usar soma binária da CPU hospedeira.
    /// Retorna `(soma, carry_out)` para `self + other + carry_in`.
    #[inline(always)]
    pub const fn full_add(self, other: Self, carry_in: Self) -> (Self, Self) {
        let (s1, c1) = self.half_add(other);
        let (sum, c2) = s1.half_add(carry_in);
        let carry_out = c1.sum_mod3(c2);
        (sum, carry_out)
    }

    /// Codificação dual-bit de 2 bits para armazenamento compacto:
    /// `0b00` -> Zero (0), `0b01` -> Positive (+1), `0b10` -> Negative (-1).
    #[inline(always)]
    pub const fn to_dual_bits(self) -> u8 {
        match self {
            Self::Zero => 0b00,
            Self::Positive => 0b01,
            Self::Negative => 0b10,
        }
    }

    /// Decodifica um par de bits dual-bit (`u8` mascarado em `0b11`) em `Trit`.
    #[inline(always)]
    pub const fn from_dual_bits(bits: u8) -> Self {
        match bits & 0b11 {
            0b01 => Self::Positive,
            0b10 => Self::Negative,
            _ => Self::Zero,
        }
    }

    /// Caractere canônico da notação ternária balanceada (`'-'`, `'0'`, `'+'`).
    #[inline(always)]
    pub const fn to_char(self) -> char {
        match self {
            Self::Negative => '-',
            Self::Zero => '0',
            Self::Positive => '+',
        }
    }

    /// Converte caractere (`'-'`/`'T'`, `'0'`, `'+'`/`'1'`) para `Option<Trit>`.
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
        // 1 + 1 + 0 = -1 com carry +1 (pois 2 = 1*3 - 1)
        assert_eq!(
            Trit::Positive.full_add(Trit::Positive, Trit::Zero),
            (Trit::Negative, Trit::Positive)
        );
        // -1 + -1 + -1 = 0 com carry -1 (pois -3 = -1*3 + 0)
        assert_eq!(
            Trit::Negative.full_add(Trit::Negative, Trit::Negative),
            (Trit::Zero, Trit::Negative)
        );
        // 1 + -1 + 0 = 0 com carry 0
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
