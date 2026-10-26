//! Aritmética de Ponto Fixo Ternário Balanceado (`TernaryFixed`).
//!
//! Substitui o formato binário `fixed_t` (`16.16`, escala $2^{16} = 65.536$) do Doom
//! por uma partição simétrica na `Word21`:
//! - **11 trits inteiros** (capacidade inteira de $[-88.573, +88.573]$).
//! - **10 trits fracionários** (fator de escala $3^{10} = 59.049$, resolução de $\approx 1{,}69 \times 10^{-5}$).
//!
//! Na lógica ternária balanceada:
//! - A negação é puramente simétrica (`-x`), sem o viés assimétrico do complemento de dois.
//! - O truncamento/arredondamento de `10` trits fracionários por divisão ternária balanceada
//!   arredonda naturalmente para o valor representável mais próximo, reduzindo o erro acumulado
//!   em projeções BSP e raycasting do motor Doom.

use crate::trit::Trit;
use crate::word::{wrap_balanced_i128, Word21, HALF_POW3, POW3};
use core::fmt;
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Número de trits reservados para a parte inteira na `Word21`.
pub const INT_TRITS: usize = 11;

/// Número de trits reservados para a parte fracionária na `Word21`.
pub const FRAC_TRITS: usize = 10;

/// Fator de escala fracionário ternário: $3^{10} = 59.049$ (equivalente ternário de `FRACUNIT`).
pub const FRAC_SCALE: i64 = POW3[FRAC_TRITS]; // 59_049

/// Metade do fator de escala ($29.524$), limite máximo da parte fracionária pura balanceada.
pub const HALF_FRAC_SCALE: i64 = HALF_POW3[FRAC_TRITS]; // 29_524

/// Valor `16.16` binário clássico do Doom (`FRACUNIT = 65536`).
pub const DOOM_BINARY_FRACUNIT: i64 = 65_536;

/// Ponto fixo ternário balanceado (`11.10` trits em uma `Word21`).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TernaryFixed(Word21);

impl TernaryFixed {
    /// Zero em ponto fixo (`0.0`).
    pub const ZERO: Self = Self(Word21::ZERO);

    /// Unidade positiva em ponto fixo (`+1.0` = $+3^{10} = +59.049$).
    pub const ONE: Self = Self(Word21::new(FRAC_SCALE));

    /// Unidade negativa em ponto fixo (`-1.0` = $-3^{10} = -59.049$).
    pub const NEG_ONE: Self = Self(Word21::new(-FRAC_SCALE));

    /// Cria um `TernaryFixed` diretamente a partir da `Word21` bruta já escalada.
    #[inline(always)]
    pub const fn from_raw(word: Word21) -> Self {
        Self(word)
    }

    /// Cria um `TernaryFixed` a partir de um inteiro escalado bruto (`raw_val`).
    #[inline(always)]
    pub const fn from_raw_i64(raw_val: i64) -> Self {
        Self(Word21::new(raw_val))
    }

    /// Retorna a `Word21` bruta subjacente.
    #[inline(always)]
    pub const fn raw_word(self) -> Word21 {
        self.0
    }

    /// Retorna o valor escalado bruto como `i64`.
    #[inline(always)]
    pub const fn raw_i64(self) -> i64 {
        self.0.as_i64()
    }

    /// Constrói um `TernaryFixed` a partir de um número inteiro (`n * 3^10`).
    #[inline(always)]
    pub const fn from_int(n: i64) -> Self {
        Self(Word21::new(n * FRAC_SCALE))
    }

    /// Extrai a parte inteira arredondada de acordo com a partição ternária balanceada
    /// (descartando os 10 trits fracionários menos significativos por deslocamento físico nos fios)
    /// e a parte fracionária em `[-29.524, +29.524]`.
    #[inline]
    pub fn split_balanced(self) -> (i64, i64) {
        let int_part = self.0.shift_trits(-10).as_i64();
        let frac_part = self.0.as_i64() - int_part * FRAC_SCALE;
        (int_part, frac_part)
    }

    /// Retorna a parte inteira truncada em direção a zero (para compatibilidade C/Doom).
    #[inline(always)]
    pub const fn to_int_trunc(self) -> i64 {
        self.0.as_i64() / FRAC_SCALE
    }

    /// Retorna a parte inteira pelo truncamento nativo de 10 trits (arredondamento simétrico).
    #[inline(always)]
    pub fn to_int_balanced(self) -> i64 {
        self.0.shift_trits(-10).as_i64()
    }

    /// Converte de `f64` para ponto fixo ternário `11.10`.
    #[inline]
    pub fn from_f64(val: f64) -> Self {
        let scaled = (val * (FRAC_SCALE as f64)).round() as i64;
        Self::from_raw_i64(scaled)
    }

    /// Converte o ponto fixo ternário `11.10` para `f64`.
    #[inline]
    pub fn to_f64(self) -> f64 {
        (self.0.as_i64() as f64) / (FRAC_SCALE as f64)
    }

    /// Converte um `fixed_t` binário `16.16` do Doom (`i32`) para `TernaryFixed` (`11.10` trits).
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

    /// Converte `TernaryFixed` (`11.10` trits) de volta para `fixed_t` binário `16.16` do Doom (`i32`).
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

    /// Sinal tricótomo direto (`Negative`, `Zero`, `Positive`).
    #[inline(always)]
    pub const fn sign_trit(self) -> Trit {
        self.0.sign_trit()
    }

    /// Multiplicação em ponto fixo ternário (`FixedMul` do Doom):
    /// Opera via multiplicador físico de portas ternárias `21x21 -> 42` trits (`gate_shift_and_add_mul42`)
    /// e deslocamento por fios de 10 posições de trits (`prod42[10..31]`), que em ternário balanceado
    /// realiza arredondamento simétrico exato para o mais próximo sem somador extra.
    #[inline]
    pub fn fixed_mul(self, rhs: Self) -> Self {
        self.fixed_mul_with_rem(rhs).0
    }

    /// Multiplicação em ponto fixo retornando também os 10 trits fracionários menos significativos
    /// descartados (`prod42[0..10]`) para preencher o registrador Multiplicador/Quociente (`R`) da CPU Setun.
    #[inline]
    pub fn fixed_mul_with_rem(self, rhs: Self) -> (Self, Word21) {
        let (prod42, _) = self.0.gate_shift_and_add_mul42(rhs.0);
        let mut mid21 = [Trit::Zero; 21];
        mid21.copy_from_slice(&prod42[FRAC_TRITS..(FRAC_TRITS + 21)]);

        let mut low_rem = [Trit::Zero; 21];
        low_rem[0..FRAC_TRITS].copy_from_slice(&prod42[0..FRAC_TRITS]);

        let result_word = Word21::from_trits(&mid21);
        let rem_word = Word21::from_trits(&low_rem);

        // Verificação matemática estrita contra divergência silenciosa:
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
            "Divergencia no deslocamento de 10 trits do multiplicador TernaryFixed"
        );
        assert_eq!(
            rem_word.as_i64(),
            expected_rem as i64,
            "Divergencia no resto fracionario de 10 trits do multiplicador TernaryFixed"
        );

        (Self(result_word), rem_word)
    }

    /// Divisão em ponto fixo ternário (`FixedDiv`) operando via algoritmo não-restaurador
    /// balanceado por deslocamento físico de fios e somador `Word21::gate_ripple_carry_add`,
    /// sem divisão `/` da CPU hospedeira.
    /// Retorna `Option<(quociente, propagacoes_carry)>` ou `None` se `rhs == 0`.
    pub fn gate_fixed_div(self, rhs: Self) -> Option<(Self, u32)> {
        if rhs.0.as_i64() == 0 {
            return None;
        }
        let denom = rhs.0.as_i64() as i128;
        // O numerador escalado por 3^10 é um deslocamento físico de 10 trits para a esquerda
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

    /// Divisão verificada em ponto fixo ternário (`FixedDiv`):
    /// Retorna `None` se `rhs == 0` (proibindo falha silenciosa na CPU).
    #[inline(always)]
    pub fn checked_fixed_div(self, rhs: Self) -> Option<Self> {
        self.gate_fixed_div(rhs).map(|(q, _)| q)
    }

    /// Divisão em ponto fixo ternário (`FixedDiv` do Doom):
    /// Calcula `(self * 3^10) / rhs` e aborta explicitamente se `rhs == 0` (use `checked_fixed_div`
    /// quando desejar capturar a armadilha de divisão por zero).
    #[inline]
    pub fn fixed_div(self, rhs: Self) -> Self {
        self.checked_fixed_div(rhs)
            .expect("Divisao por zero em TernaryFixed::fixed_div (proibido falhar silenciosamente)")
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
        // Simetria estrita: (-a) * b == -(a * b)
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

        // Conversão ida e volta para fixed_t 16.16 do Doom
        let doom_val: i32 = -(42 * 65536 + 32768); // -42.5 em 16.16
        let tf = TernaryFixed::from_doom_fixed16(doom_val);
        assert!((tf.to_f64() - (-42.5)).abs() < 1e-4);
        let back = tf.to_doom_fixed16();
        assert!((back - doom_val).abs() <= 2);
    }
}
