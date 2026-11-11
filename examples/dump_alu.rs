// Temporary instrument: ground truth for the ALU verification.
use setun_doom::rebel6::word::Word24;

fn wrap(v: i64) -> i64 {
    let m = 282_429_536_481i64;
    let h = 141_214_768_240i64;
    let mut r = v % m;
    if r > h { r -= m; } else if r < -h { r += m; }
    r
}

fn main() {
    // deterministic pseudo random
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut rnd = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as i64
    };
    let mut ops = Vec::new();
    for op in 0..15 {
        for _ in 0..400 {
            let a = (rnd() % (2 * 141_214_768_240u64 as i64 + 1)) - 141_214_768_240;
            let b = (rnd() % (2 * 141_214_768_240u64 as i64 + 1)) - 141_214_768_240;
            ops.push((op, a, b));
        }
    }
    for (op, a, b) in ops {
        let wa = Word24::new(a);
        let wb = Word24::new(b);
        let (res, eq, lt) = match op {
            0 => (wrap(wa.raw() + wb.raw()), wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            1 => (wrap(wa.raw() - wb.raw()), wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            2 => (if wa.raw() < wb.raw() { 1 } else { 0 }, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            3 => (wa.raw().min(wb.raw()), wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            4 => (wa.raw().max(wb.raw()), wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            5 => (wa.raw().min(wb.raw()), wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            6 => (wa.to_i32().wrapping_add(wb.to_i32()) as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            7 => (wa.to_i32().wrapping_sub(wb.to_i32()) as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            8 => ((wa.to_i32().wrapping_shl((wb.to_i32() as u32) & 31)) as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            9 => (((wa.to_i32() as u32).wrapping_shr((wb.to_i32() as u32) & 31)) as i32 as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            10 => ((wa.to_i32().wrapping_shr((wb.to_i32() as u32) & 31)) as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            11 => (if (wa.to_i32() as u32) < (wb.to_i32() as u32) { 1 } else { 0 }, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            12 => ((wa.to_i32() | wb.to_i32()) as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            13 => ((wa.to_i32() ^ wb.to_i32()) as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            14 => ((wa.to_i32() & wb.to_i32()) as i64, wa.raw() == wb.raw(), wa.raw() < wb.raw()),
            _ => (0, false, false),
        };
        println!("{op}|{a}|{b}|{res}|{eq}|{lt}");
    }
}
