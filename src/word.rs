//! Palavras ternárias balanceadas de 21 e 27 trits, trytes de 6 trits e empacotamento
//! dual-bit / aritmética direta em `i64` para o caminho crítico da CPU.
//!
//! Uma palavra de 21 trits (`Word21`) cobre $3^{21} = 10.460.353.203$ estados,
//! distribuídos simetricamente em $[-5.230.176.601, +5.230.176.601]$, superando
//! com folga a capacidade de 32 bits binários ($2^{32} = 4.294.967.296$).

use crate::trit::Trit;
use core::fmt;
use core::ops::{Add, Div, Mul, Neg, Sub};

/// Tabelas pré-computadas de potências de 3 (`3^0` até `3^27`).
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

/// Metade inteira de `3^N` (`(3^N - 1) / 2`), definindo o limite máximo positivo de `N` trits.
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

/// Envelopa um valor arbitrário `i64` para o domínio ternário balanceado de `N` trits
/// em `[-HALF_POW3[N], +HALF_POW3[N]]` preservando a congruência módulo `3^N`.
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

/// Envelopamento rápido de 128 bits para 21 trits (usado após multiplicações largas).
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

/// Converte um inteiro `i64` em um array de `N` trits balanceados (índice 0 = trit menos significativo)
/// pelo algoritmo clássico de divisão sucessiva com resto balanceado em `{-1, 0, +1}`.
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

/// Converte um slice ou array de `N` trits balanceados (índice 0 = trit menos significativo)
/// para o inteiro equivalente `i64`.
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

/// Extrai uma sub-fatia contígua de `LEN` trits começando na posição ` OFFSET` (0-indexada)
/// diretamente a partir do valor numérico `val`, sem alocar arrays de trits.
#[inline(always)]
pub const fn extract_trit_field(val: i64, offset: usize, len: usize) -> i64 {
    // Arredondamento ternário balanceado na divisão por 3^offset:
    // val = q * 3^offset + r, onde r está em [-half_offset, +half_offset].
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

/// Empacota um array de `N` trits em representação dual-bit (`u64`), onde cada trit ocupa 2 bits:
/// bits `[2*i .. 2*i+1]` armazenam `trits[i].to_dual_bits()`.
#[inline]
pub fn pack_dual_bits<const N: usize>(trits: &[Trit; N]) -> u64 {
    let mut packed: u64 = 0;
    for (i, &t) in trits.iter().enumerate() {
        packed |= (t.to_dual_bits() as u64) << (i * 2);
    }
    packed
}

/// Desempacota um `u64` dual-bit em um array de `N` trits.
#[inline]
pub fn unpack_dual_bits<const N: usize>(packed: u64) -> [Trit; N] {
    let mut trits = [Trit::Zero; N];
    for (i, slot) in trits.iter_mut().enumerate() {
        let bits = ((packed >> (i * 2)) & 0b11) as u8;
        *slot = Trit::from_dual_bits(bits);
    }
    trits
}

/// Tryte de 6 trits balanceados ($3^6 = 729$ estados, amplitude `[-364, +364]`).
/// Utilizado para indexação de paleta de cores no framebuffer VGA 320x200 e bytes expandidos.
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

    /// Converte um byte binário `u8` (0..=255) para `Tryte6` centrado simetricamente
    /// ou mapeado diretamente. Aqui mapeamos `b as i16 - 128` em `[-128, +127]`,
    /// totalmente contido na faixa `[-364, +364]`.
    #[inline(always)]
    pub const fn from_u8_centered(byte: u8) -> Self {
        Self((byte as i16) - 128)
    }

    /// Reconverte um `Tryte6` centrado (`[-128, +127]`) para `u8` (`0..=255`).
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

    /// Converte um índice de paleta VGA (`0..=255`) preservando representação ternária direta.
    #[inline(always)]
    pub const fn from_palette_index(idx: u8) -> Self {
        Self((idx as i16) - 128)
    }

    /// Extrai o índice de paleta VGA (`0..=255`) a partir do `Tryte6`.
    #[inline(always)]
    pub const fn to_palette_index(self) -> u8 {
        // Permite tanto valores centrados [-128..127] quanto diretos modulo 256 dentro de [-364..364]
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

/// Palavra principal de 21 trits balanceados (`Setun-Doom ISA`).
///
/// - Capacidade total: $3^{21} = 10.460.353.203$ estados (> 2.4x o espaço de 32 bits binários).
/// - Amplitude simétrica: $[-5.230.176.601, +5.230.176.601]$.
/// - Armazenada internamente em `i64` alinhado no caminho crítico da CPU para execução em
///   ciclo único no processador hospedeiro, mantendo semântica aritmética ternária estrita.
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

    /// Constrói uma `Word21` envelopando o valor `i64` nos 21 trits balanceados.
    #[inline(always)]
    pub const fn new(val: i64) -> Self {
        Self(wrap_balanced::<21>(val))
    }

    /// Constrói uma `Word21` a partir de um `i32` binário (conversão exata sem overflow,
    /// pois `i32::MAX = 2_147_483_647 < 5_230_176_601`).
    #[inline(always)]
    pub const fn from_i32(val: i32) -> Self {
        Self(val as i64)
    }

    /// Constrói uma `Word21` a partir de um `i64` binário.
    #[inline(always)]
    pub const fn from_i64(val: i64) -> Self {
        Self::new(val)
    }

    /// Retorna o valor inteiro assinado `i64` subjacente.
    #[inline(always)]
    pub const fn as_i64(self) -> i64 {
        self.0
    }

    /// Converte para `i32` saturando nos limites de 32 bits binários caso exceda `i32::MIN..=i32::MAX`.
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

    /// Sinal tricótomo intrínseco da palavra (`Trit::Negative`, `Trit::Zero` ou `Trit::Positive`).
    /// Substitui flags artificiais de CPU (Zero/Sign/Carry) por inspeção direta da palavra.
    #[inline(always)]
    pub const fn sign_trit(self) -> Trit {
        Trit::from_i64_sign(self.0)
    }

    /// Inversão simples de todos os 21 trits (negação simétrica instantânea sem soma de +1).
    #[inline(always)]
    pub const fn neg_trits(self) -> Self {
        Self(-self.0)
    }

    /// Inversor físico de 21 portas `Trit::invert` operando trit-a-trit em `[Trit; 21]`.
    #[inline]
    pub fn gate_invert_trits(self) -> Self {
        let trits = self.to_trits();
        let mut out = [Trit::Zero; 21];
        for i in 0..21 {
            out[i] = trits[i].invert();
        }
        let res = Self::from_trits(&out);
        assert_eq!(res.0, -self.0, "Divergencia no inversor de portas ternarias");
        res
    }

    /// Somador Ripple-Carry de 21 estágios construído exclusivamente com portas `Trit::full_add`.
    /// Retorna `(palavra_soma, carry_out_trit_21, contagem_propagacoes_carry)`.
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
            "Divergencia no somador Ripple-Carry de 21 trits"
        );
        (res, carry, carry_props)
    }

    /// Multiplicador físico ternário balanceado Shift-and-Add/Subtract de `21 x 21 -> 42` trits,
    /// operando exclusivamente com portas `Trit::t_mul` e `Trit::full_add`.
    /// Retorna `(produto_42_trits, propagacoes_carry)`.
    #[inline]
    pub fn gate_shift_and_add_mul42(self, rhs: Self) -> ([Trit; 42], u32) {
        let a = self.to_trits();
        let b = rhs.to_trits();
        let mut acc = [Trit::Zero; 42];
        let mut carry_props = 0u32;

        for i in 0..21 {
            let mult_trit = b[i];
            if mult_trit == Trit::Zero {
                // Em ternário balanceado, trits Zero pulam o estágio sem comutar portas
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

    /// Comparador de magnitude ternário de 21 trits (varredura do trit mais significativo `20` até `0`
    /// usando a porta elementar `Trit::cmp_trit`).
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
            "Divergencia no comparador de magnitude ternario de 21 trits"
        );
        result
    }

    /// Divisor ternário balanceado não-restaurador de 21 passos operando exclusivamente
    /// por deslocamento físico de fios, somador `gate_ripple_carry_add` e lógica ternária,
    /// sem recorrer a divisão `/` ou `%` da CPU hospedeira.
    /// Retorna `Option<(quociente, resto, propagacoes_carry)>`.
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

    /// Divisão inteira verificada via divisor ternário não-restaurador de portas lógicas
    /// (retorna `None` se `rhs == 0`, proibindo falha silenciosa).
    #[inline(always)]
    pub fn checked_div_rem(self, rhs: Self) -> Option<(Self, Self)> {
        self.gate_non_restoring_div_rem(rhs).map(|(q, r, _)| (q, r))
    }

    /// Deslocamento físico de fios de `shift` trits em `[Trit; 21]`:
    /// - `shift > 0`: desloca para a esquerda (multiplica por $3^{\text{shift}}$) com descarte do overflow nos 21 fios.
    /// - `shift < 0`: desloca para a direita (descarta os $|\text{shift}|$ trits menos significativos,
    ///   realizando arredondamento simétrico exato para o mais próximo sem necessidade de somador extra).
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

    /// Decompõe a palavra nos 21 trits individuais (índice 0 = trit menos significativo).
    #[inline]
    pub fn to_trits(self) -> [Trit; 21] {
        i64_to_trits::<21>(self.0)
    }

    /// Constrói uma `Word21` a partir de 21 trits individuais.
    #[inline]
    pub fn from_trits(trits: &[Trit; 21]) -> Self {
        Self::new(trits_to_i64::<21>(trits))
    }

    /// Empacota a palavra em codificação dual-bit de 42 bits dentro de um `u64`.
    #[inline]
    pub fn to_dual_bits(self) -> u64 {
        pack_dual_bits::<21>(&self.to_trits())
    }

    /// Reconstrói a `Word21` a partir da codificação dual-bit de 42 bits.
    #[inline]
    pub fn from_dual_bits(packed: u64) -> Self {
        Self::from_trits(&unpack_dual_bits::<21>(packed))
    }

    /// Operação lógica trit-a-trit de multiplicação (XOR ternário / produto de sinais).
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

    /// Operação lógica trit-a-trit de mínimo (AND ternário de Kleene).
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

    /// Operação lógica trit-a-trit de máximo (OR ternário de Kleene).
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

    /// Votação Majoritária trit-a-trit entre 3 palavras (`majv.t` do artigo REBEL-6, IEEE ISMVL 2025).
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

    /// Votação Minoritária trit-a-trit entre 3 palavras (`minv.t` do artigo REBEL-6, IEEE ISMVL 2025).
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

    /// Adição Limiar trit-a-trit entre 2 palavras (`SAT` / Operação Básica `B21` `000+-+` da Setun-70).
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

    /// Inibição / Coalescência trit-a-trit (`SIT` / Operação Básica `B20` `000+-0` da Setun-70):
    /// Calcula `self + other * (1 - |self|)` em cada um dos 21 trits.
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

    /// Conversor de base em hardware $2 \to 3$ ($p \to p+1$) pelo algoritmo recorrente de
    /// J. Ramil Alvarez (*"Простые алгоритмы перевода p -> p-1 и p -> p+1"*, Setun Lab, 1974).
    ///
    /// Converte um inteiro binário `i32` em complemento de dois para `[Trit; 21]` usando
    /// exclusivamente deslocamento físico de 1 trit ($3A$), inversão de fios ($-A$) e o somador
    /// `Trit::full_add` com `bit_k` injetado no `carry_in` ($2A + b_k = 3A - A + b_k$),
    /// sem usar divisão `/ 3` ou módulo `% 3` da CPU hospedeira.
    pub fn alvarez_from_i32_gates(val: i32) -> ([Trit; 21], u32) {
        let mut acc = [Trit::Zero; 21];
        let mut total_carries = 0u32;
        if val < 0 {
            acc[0] = Trit::Negative;
        }
        for bit_idx in (0..31).rev() {
            let bit = ((val >> bit_idx) & 1) != 0;
            let carry_in = if bit { Trit::Positive } else { Trit::Zero };

            // Deslocamento físico de 1 trit para a esquerda (3 * A) e inversão (-A)
            let mut shifted = [Trit::Zero; 21];
            let mut neg_acc = [Trit::Zero; 21];
            for i in 0..21 {
                if i > 0 {
                    shifted[i] = acc[i - 1];
                }
                neg_acc[i] = acc[i].invert();
            }

            // Soma ripple-carry de 21 estágios: (3 * A) + (-A) + bit_k = 2 * A + bit_k
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
            "Divergencia no conversor de base 2->3 de Ramil Alvarez"
        );
        (acc, total_carries)
    }

    /// Comparação tricótoma (`cmp.t` / operador `<=>` do artigo REBEL-6):
    /// Opera via comparador de magnitude de 21 trits (`gate_cmp_t`).
    /// Retorna `Word21::ONE` (`+1`) se `self > rhs`, `Word21::ZERO` (`0`) se `self == rhs`
    /// e `Word21::NEG_ONE` (`-1`) se `self < rhs`.
    #[inline]
    pub fn cmp_t(self, rhs: Self) -> Self {
        Self(self.gate_cmp_t(rhs).as_i64())
    }

    /// Conta quantos dos 21 trits mudaram de estado entre `self` e `next`
    /// (métrica de *Digit Flips* ternários da Seção IV.B do artigo REBEL-6).
    /// Também retorna quantas dessas transições foram *half-swings* (`-1 <-> 0` ou `0 <-> +1`,
    /// que consomem menos energia dinâmica) vs *full-swings* (`-1 <-> +1`).
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

    /// Conta quantos bits mudariam em um registrador binário de 32 bits em complemento de dois (`RV32I`)
    /// para a mesma transição de valor inteiro `self -> next` (usando `(a_i32 ^ b_i32).count_ones()`).
    #[inline(always)]
    pub const fn count_binary32_flips(self, next: Self) -> u32 {
        let a32 = self.to_i32_saturating();
        let b32 = next.to_i32_saturating();
        (a32 ^ b32).count_ones()
    }

    /// Analisa uma string de trits balanceados (ex: `"+0-+"`, MSB primeiro) em `Word21`.
    pub fn from_balanced_str(s: &str) -> Result<Self, &'static str> {
        let mut acc: i64 = 0;
        let mut count = 0usize;
        for ch in s.chars() {
            if ch == '_' || ch.is_whitespace() {
                continue;
            }
            let t = Trit::from_char(ch).ok_or("Caractere ternario invalido")?;
            acc = acc * 3 + t.as_i64();
            count += 1;
            if count > 21 {
                return Err("Excede 21 trits");
            }
        }
        Ok(Self::new(acc))
    }

    /// Formata a palavra como string de 21 caracteres ternários balanceados (MSB primeiro).
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
            "Divergencia no multiplicador Shift-and-Add de 21 trits"
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
            .expect("Divisao por zero em Word21 (proibido falhar silenciosamente)");
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

/// Palavra estendida de 27 trits balanceados (`Word27`).
///
/// - Capacidade: $3^{27} = 7.625.597.484.987$ estados (amplitude `[-3.812.798.742.493, +3.812.798.742.493]`).
/// - Útil para acumuladores duplos, produtos intermediários de alta precisão e instruções longas.
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

            // Inversão de cada trit individual deve corresponder exatamente a -val
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
        // MAX_VAL + 1 dá a volta para MIN_VAL em aritmética modular balanceada
        let max = Word21::new(Word21::MAX_VAL);
        let wrapped = max + Word21::ONE;
        assert_eq!(wrapped.as_i64(), Word21::MIN_VAL);

        let min = Word21::new(Word21::MIN_VAL);
        let wrapped_back = min - Word21::ONE;
        assert_eq!(wrapped_back.as_i64(), Word21::MAX_VAL);
    }
}
