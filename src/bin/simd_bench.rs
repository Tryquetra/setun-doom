//! Measures the AVX2 bitsliced ternary kernels against their scalar versions.
//!
//! Two working sets are reported: 1 Mi trits (memory-bound) and 32 Ki trits
//! (L1-resident, compute-bound).

use std::hint::black_box;
use std::time::Instant;

use setun_doom::simd;

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

fn measure(iterations: usize, mut f: impl FnMut()) -> f64 {
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    start.elapsed().as_secs_f64() / iterations as f64
}

fn report(name: &str, trits: usize, scalar: f64, vector: f64) {
    println!(
        "{name:<8} scalar {:>9.3} us | avx2 {:>9.3} us | {:>6.2}x | {:>8.2} Gtrit/s",
        scalar * 1e6,
        vector * 1e6,
        scalar / vector,
        trits as f64 / vector / 1e9
    );
}

fn run(trits: usize, iterations: usize) {
    let words = trits / 64;
    println!(
        "\nworking set: {trits} trits ({words} words, {} KiB per plane) | iterations: {iterations}",
        words * 8 / 1024
    );
    let (ap, an) = fill(words, 1);
    let (bp, bn) = fill(words, 2);
    let mut dp = vec![0u64; words];
    let mut dn = vec![0u64; words];

    let scalar = measure(iterations, || {
        for i in 0..words {
            dp[i] = black_box(ap[i] & bp[i]);
            dn[i] = black_box(an[i] | bn[i]);
        }
    });
    let vector = measure(iterations, || {
        simd::and(&mut dp, &mut dn, &ap, &an, &bp, &bn);
        black_box(&dp);
    });
    report("AND.T", trits, scalar, vector);

    let scalar = measure(iterations, || {
        for i in 0..words {
            dp[i] = black_box(ap[i] | bp[i]);
            dn[i] = black_box(an[i] & bn[i]);
        }
    });
    let vector = measure(iterations, || {
        simd::or(&mut dp, &mut dn, &ap, &an, &bp, &bn);
        black_box(&dp);
    });
    report("OR.T", trits, scalar, vector);

    let scalar = measure(iterations, || {
        for i in 0..words {
            dp[i] = black_box((ap[i] & bn[i]) | (an[i] & bp[i]));
            dn[i] = black_box((ap[i] & bp[i]) | (an[i] & bn[i]));
        }
    });
    let vector = measure(iterations, || {
        simd::xor(&mut dp, &mut dn, &ap, &an, &bp, &bn);
        black_box(&dp);
    });
    report("XOR.T", trits, scalar, vector);

    let mut sp = ap.clone();
    let mut sn = an.clone();
    let scalar = measure(iterations, || {
        for i in 0..words {
            core::mem::swap(&mut sp[i], &mut sn[i]);
        }
        black_box(&sp);
    });
    let mut vp = ap.clone();
    let mut vn = an.clone();
    let vector = measure(iterations, || {
        simd::negate(&mut vp, &mut vn);
        black_box(&vp);
    });
    report("NEG", trits, scalar, vector);
}

fn main() {
    println!("AVX2 available: {}", simd::avx2_available());
    run(1 << 20, 200);
    run(1 << 15, 20_000);
}
