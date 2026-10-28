//! AVX2 bitsliced kernels for balanced ternary.
//!
//! A trit is stored in two bit planes: `p = 1` marks `+1`, `n = 1` marks `-1`,
//! and `p = n = 0` marks `0`. One `__m256i` pair holds 256 trits. The scalar
//! functions use the same layout, so results are directly comparable.
//!
//! These kernels cover tritwise logic. A full adder needs carry propagation
//! across trit positions and is not a single vector instruction.

use core::arch::x86_64::{
    __m128i, __m256i, _mm256_add_epi64, _mm256_and_si256, _mm256_castsi256_pd, _mm256_cmpgt_epi64,
    _mm256_i64gather_epi32, _mm256_loadu_si256, _mm256_movemask_pd, _mm256_or_si256,
    _mm256_set1_epi64x, _mm256_setzero_si256, _mm256_storeu_si256, _mm_storeu_si128,
};

/// Returns true when the host supports AVX2.
#[must_use]
pub fn avx2_available() -> bool {
    std::is_x86_feature_detected!("avx2")
}

/// Swaps the `p` and `n` planes, which negates every trit.
pub fn negate(p: &mut [u64], n: &mut [u64]) {
    if avx2_available() {
        // SAFETY: AVX2 was detected at runtime and both slices have the same length.
        unsafe { negate_avx2(p, n) };
    } else {
        negate_scalar(p, n);
    }
}

/// Tritwise minimum (`AND.T`): `p = ap & bp`, `n = an | bn`.
pub fn and(dst_p: &mut [u64], dst_n: &mut [u64], ap: &[u64], an: &[u64], bp: &[u64], bn: &[u64]) {
    if avx2_available() {
        // SAFETY: AVX2 was detected at runtime and all slices have the same length.
        unsafe { and_avx2(dst_p, dst_n, ap, an, bp, bn) };
    } else {
        and_scalar(dst_p, dst_n, ap, an, bp, bn);
    }
}

/// Tritwise maximum (`OR.T`): `p = ap | bp`, `n = an & bn`.
pub fn or(dst_p: &mut [u64], dst_n: &mut [u64], ap: &[u64], an: &[u64], bp: &[u64], bn: &[u64]) {
    if avx2_available() {
        // SAFETY: AVX2 was detected at runtime and all slices have the same length.
        unsafe { or_avx2(dst_p, dst_n, ap, an, bp, bn) };
    } else {
        or_scalar(dst_p, dst_n, ap, an, bp, bn);
    }
}

/// Tritwise `-(a * b)` (`XOR.T`).
pub fn xor(dst_p: &mut [u64], dst_n: &mut [u64], ap: &[u64], an: &[u64], bp: &[u64], bn: &[u64]) {
    if avx2_available() {
        // SAFETY: AVX2 was detected at runtime and all slices have the same length.
        unsafe { xor_avx2(dst_p, dst_n, ap, an, bp, bn) };
    } else {
        xor_scalar(dst_p, dst_n, ap, an, bp, bn);
    }
}

/// Converts `Tryte6` framebuffer cells into packed RGBA pixels using the palette.
///
/// Returns true when the AVX2 path was used. Out-of-range cells fall back to the
/// scalar `Tryte6::to_palette_index` rule.
pub fn palette_rgba(dst: &mut [u8], cells: &[i64], palette: &[[u8; 4]; 256]) -> bool {
    if avx2_available() {
        // SAFETY: AVX2 was detected at runtime; bounds are checked inside the kernel.
        unsafe { palette_rgba_avx2(dst, cells, palette) };
        true
    } else {
        palette_rgba_scalar(dst, cells, palette);
        false
    }
}

fn palette_rgba_scalar(dst: &mut [u8], cells: &[i64], palette: &[[u8; 4]; 256]) {
    let count = cells.len().min(dst.len() / 4);
    for (i, &value) in cells.iter().take(count).enumerate() {
        let index = palette_index(value);
        dst[i * 4..i * 4 + 4].copy_from_slice(&palette[index]);
    }
}

#[inline]
fn palette_index(value: i64) -> usize {
    if (-128..=127).contains(&value) {
        (value + 128) as usize
    } else {
        value.rem_euclid(256) as usize
    }
}

#[target_feature(enable = "avx2")]
unsafe fn palette_rgba_avx2(dst: &mut [u8], cells: &[i64], palette: &[[u8; 4]; 256]) {
    let count = cells.len().min(dst.len() / 4);
    let base = palette.as_ptr().cast::<i32>();
    let mut i = 0;
    while i + 4 <= count {
        // SAFETY: `i + 4 <= count`, so the 4 qword load and 16-byte store are in bounds.
        let values = unsafe { _mm256_loadu_si256(cells.as_ptr().add(i).cast::<__m256i>()) };
        let indices = _mm256_add_epi64(values, _mm256_set1_epi64x(128));
        let high = _mm256_cmpgt_epi64(indices, _mm256_set1_epi64x(255));
        let low = _mm256_cmpgt_epi64(_mm256_setzero_si256(), indices);
        let bad = _mm256_or_si256(high, low);
        if _mm256_movemask_pd(_mm256_castsi256_pd(bad)) == 0 {
            // SAFETY: all indices are in `0..=255`; the palette has 256 entries.
            let rgba = unsafe { _mm256_i64gather_epi32(base, indices, 4) };
            unsafe {
                _mm_storeu_si128(dst.as_mut_ptr().add(i * 4).cast::<__m128i>(), rgba);
            }
        } else {
            for j in 0..4 {
                let index = palette_index(cells[i + j]);
                dst[(i + j) * 4..(i + j) * 4 + 4].copy_from_slice(&palette[index]);
            }
        }
        i += 4;
    }
    for j in i..count {
        let index = palette_index(cells[j]);
        dst[j * 4..j * 4 + 4].copy_from_slice(&palette[index]);
    }
}

fn negate_scalar(p: &mut [u64], n: &mut [u64]) {
    for (a, b) in p.iter_mut().zip(n.iter_mut()) {
        core::mem::swap(a, b);
    }
}

fn and_scalar(
    dst_p: &mut [u64],
    dst_n: &mut [u64],
    ap: &[u64],
    an: &[u64],
    bp: &[u64],
    bn: &[u64],
) {
    for i in 0..dst_p.len() {
        dst_p[i] = ap[i] & bp[i];
        dst_n[i] = an[i] | bn[i];
    }
}

fn or_scalar(dst_p: &mut [u64], dst_n: &mut [u64], ap: &[u64], an: &[u64], bp: &[u64], bn: &[u64]) {
    for i in 0..dst_p.len() {
        dst_p[i] = ap[i] | bp[i];
        dst_n[i] = an[i] & bn[i];
    }
}

fn xor_scalar(
    dst_p: &mut [u64],
    dst_n: &mut [u64],
    ap: &[u64],
    an: &[u64],
    bp: &[u64],
    bn: &[u64],
) {
    for i in 0..dst_p.len() {
        dst_p[i] = (ap[i] & bn[i]) | (an[i] & bp[i]);
        dst_n[i] = (ap[i] & bp[i]) | (an[i] & bn[i]);
    }
}

#[target_feature(enable = "avx2")]
unsafe fn negate_avx2(p: &mut [u64], n: &mut [u64]) {
    let len = p.len().min(n.len());
    let mut i = 0;
    while i + 4 <= len {
        // SAFETY: `i + 4 <= len`, both slices are valid for reads and writes.
        let pv = unsafe { _mm256_loadu_si256(p.as_ptr().add(i).cast::<__m256i>()) };
        let nv = unsafe { _mm256_loadu_si256(n.as_ptr().add(i).cast::<__m256i>()) };
        unsafe {
            _mm256_storeu_si256(p.as_mut_ptr().add(i).cast::<__m256i>(), nv);
            _mm256_storeu_si256(n.as_mut_ptr().add(i).cast::<__m256i>(), pv);
        }
        i += 4;
    }
    while i < len {
        core::mem::swap(&mut p[i], &mut n[i]);
        i += 1;
    }
}

#[target_feature(enable = "avx2")]
unsafe fn and_avx2(
    dst_p: &mut [u64],
    dst_n: &mut [u64],
    ap: &[u64],
    an: &[u64],
    bp: &[u64],
    bn: &[u64],
) {
    let len = dst_p.len();
    let mut i = 0;
    while i + 4 <= len {
        // SAFETY: `i + 4 <= len`, all slices are valid for reads, destinations for writes.
        let apv = unsafe { _mm256_loadu_si256(ap.as_ptr().add(i).cast::<__m256i>()) };
        let anv = unsafe { _mm256_loadu_si256(an.as_ptr().add(i).cast::<__m256i>()) };
        let bpv = unsafe { _mm256_loadu_si256(bp.as_ptr().add(i).cast::<__m256i>()) };
        let bnv = unsafe { _mm256_loadu_si256(bn.as_ptr().add(i).cast::<__m256i>()) };
        let p = _mm256_and_si256(apv, bpv);
        let n = _mm256_or_si256(anv, bnv);
        unsafe {
            _mm256_storeu_si256(dst_p.as_mut_ptr().add(i).cast::<__m256i>(), p);
            _mm256_storeu_si256(dst_n.as_mut_ptr().add(i).cast::<__m256i>(), n);
        }
        i += 4;
    }
    while i < len {
        dst_p[i] = ap[i] & bp[i];
        dst_n[i] = an[i] | bn[i];
        i += 1;
    }
}

#[target_feature(enable = "avx2")]
unsafe fn or_avx2(
    dst_p: &mut [u64],
    dst_n: &mut [u64],
    ap: &[u64],
    an: &[u64],
    bp: &[u64],
    bn: &[u64],
) {
    let len = dst_p.len();
    let mut i = 0;
    while i + 4 <= len {
        // SAFETY: `i + 4 <= len`, all slices are valid for reads, destinations for writes.
        let apv = unsafe { _mm256_loadu_si256(ap.as_ptr().add(i).cast::<__m256i>()) };
        let anv = unsafe { _mm256_loadu_si256(an.as_ptr().add(i).cast::<__m256i>()) };
        let bpv = unsafe { _mm256_loadu_si256(bp.as_ptr().add(i).cast::<__m256i>()) };
        let bnv = unsafe { _mm256_loadu_si256(bn.as_ptr().add(i).cast::<__m256i>()) };
        let p = _mm256_or_si256(apv, bpv);
        let n = _mm256_and_si256(anv, bnv);
        unsafe {
            _mm256_storeu_si256(dst_p.as_mut_ptr().add(i).cast::<__m256i>(), p);
            _mm256_storeu_si256(dst_n.as_mut_ptr().add(i).cast::<__m256i>(), n);
        }
        i += 4;
    }
    while i < len {
        dst_p[i] = ap[i] | bp[i];
        dst_n[i] = an[i] & bn[i];
        i += 1;
    }
}

#[target_feature(enable = "avx2")]
unsafe fn xor_avx2(
    dst_p: &mut [u64],
    dst_n: &mut [u64],
    ap: &[u64],
    an: &[u64],
    bp: &[u64],
    bn: &[u64],
) {
    let len = dst_p.len();
    let mut i = 0;
    while i + 4 <= len {
        // SAFETY: `i + 4 <= len`, all slices are valid for reads, destinations for writes.
        let apv = unsafe { _mm256_loadu_si256(ap.as_ptr().add(i).cast::<__m256i>()) };
        let anv = unsafe { _mm256_loadu_si256(an.as_ptr().add(i).cast::<__m256i>()) };
        let bpv = unsafe { _mm256_loadu_si256(bp.as_ptr().add(i).cast::<__m256i>()) };
        let bnv = unsafe { _mm256_loadu_si256(bn.as_ptr().add(i).cast::<__m256i>()) };
        let p = _mm256_or_si256(_mm256_and_si256(apv, bnv), _mm256_and_si256(anv, bpv));
        let n = _mm256_or_si256(_mm256_and_si256(apv, bpv), _mm256_and_si256(anv, bnv));
        unsafe {
            _mm256_storeu_si256(dst_p.as_mut_ptr().add(i).cast::<__m256i>(), p);
            _mm256_storeu_si256(dst_n.as_mut_ptr().add(i).cast::<__m256i>(), n);
        }
        i += 4;
    }
    while i < len {
        dst_p[i] = (ap[i] & bn[i]) | (an[i] & bp[i]);
        dst_n[i] = (ap[i] & bp[i]) | (an[i] & bn[i]);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(words: usize, seed: u64) -> (Vec<u64>, Vec<u64>) {
        let mut p = Vec::with_capacity(words);
        let mut n = Vec::with_capacity(words);
        let mut state = seed;
        for _ in 0..words {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let a = state;
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let b = state;
            p.push(a & !b);
            n.push(b & !a);
        }
        (p, n)
    }

    #[test]
    fn tritwise_truth_tables() {
        // Nine input pairs, one trit per word, packed in the low bit.
        let trits = [-1i64, 0, 1];
        for &a in &trits {
            for &b in &trits {
                let (mut ap, mut an) = (0u64, 0u64);
                if a == 1 {
                    ap = 1;
                } else if a == -1 {
                    an = 1;
                }
                let (mut bp, mut bn) = (0u64, 0u64);
                if b == 1 {
                    bp = 1;
                } else if b == -1 {
                    bn = 1;
                }
                let mut p = [0u64];
                let mut n = [0u64];
                and(&mut p, &mut n, &[ap], &[an], &[bp], &[bn]);
                let got = if p[0] == 1 {
                    1
                } else if n[0] == 1 {
                    -1
                } else {
                    0
                };
                assert_eq!(got, a.min(b), "AND.T {a} {b}");

                let mut p = [0u64];
                let mut n = [0u64];
                or(&mut p, &mut n, &[ap], &[an], &[bp], &[bn]);
                let got = if p[0] == 1 {
                    1
                } else if n[0] == 1 {
                    -1
                } else {
                    0
                };
                assert_eq!(got, a.max(b), "OR.T {a} {b}");

                let mut p = [0u64];
                let mut n = [0u64];
                xor(&mut p, &mut n, &[ap], &[an], &[bp], &[bn]);
                let got = if p[0] == 1 {
                    1
                } else if n[0] == 1 {
                    -1
                } else {
                    0
                };
                assert_eq!(got, -(a * b), "XOR.T {a} {b}");
            }
        }
    }

    #[test]
    fn avx2_matches_scalar() {
        if !avx2_available() {
            return;
        }
        let words = 64;
        let (ap, an) = fill(words, 1);
        let (bp, bn) = fill(words, 2);

        let mut sp = vec![0u64; words];
        let mut sn = vec![0u64; words];
        and_scalar(&mut sp, &mut sn, &ap, &an, &bp, &bn);
        let mut vp = vec![0u64; words];
        let mut vn = vec![0u64; words];
        and(&mut vp, &mut vn, &ap, &an, &bp, &bn);
        assert_eq!((sp, sn), (vp, vn), "AND.T");

        let mut sp = vec![0u64; words];
        let mut sn = vec![0u64; words];
        or_scalar(&mut sp, &mut sn, &ap, &an, &bp, &bn);
        let mut vp = vec![0u64; words];
        let mut vn = vec![0u64; words];
        or(&mut vp, &mut vn, &ap, &an, &bp, &bn);
        assert_eq!((sp, sn), (vp, vn), "OR.T");

        let mut sp = vec![0u64; words];
        let mut sn = vec![0u64; words];
        xor_scalar(&mut sp, &mut sn, &ap, &an, &bp, &bn);
        let mut vp = vec![0u64; words];
        let mut vn = vec![0u64; words];
        xor(&mut vp, &mut vn, &ap, &an, &bp, &bn);
        assert_eq!((sp, sn), (vp, vn), "XOR.T");

        let mut sp = ap.clone();
        let mut sn = an.clone();
        negate_scalar(&mut sp, &mut sn);
        let mut vp = ap.clone();
        let mut vn = an.clone();
        negate(&mut vp, &mut vn);
        assert_eq!((sp, sn), (vp, vn), "negate");
    }
    #[test]
    fn palette_rgba_matches_scalar() {
        let mut palette = [[0u8; 4]; 256];
        for (i, entry) in palette.iter_mut().enumerate() {
            *entry = [
                i as u8,
                (i as u8).wrapping_mul(3),
                (i as u8).wrapping_mul(7),
                255,
            ];
        }
        let cells: Vec<i64> = (0..4096)
            .map(|i| match i % 17 {
                0 => 200,
                1 => -200,
                _ => ((i % 256) as i64) - 128,
            })
            .collect();
        let mut scalar = vec![0u8; cells.len() * 4];
        let mut vector = vec![0u8; cells.len() * 4];
        palette_rgba_scalar(&mut scalar, &cells, &palette);
        palette_rgba(&mut vector, &cells, &palette);
        assert_eq!(scalar, vector);
    }
}
