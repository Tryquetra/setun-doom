#[inline(always)]
pub fn sbi_call(eid: usize, fid: usize, arg0: usize, arg1: usize, arg2: usize) -> (usize, usize) {
    let err: usize;
    let val: usize;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") arg0 => err,
            inlateout("a1") arg1 => val,
            in("a2") arg2,
            in("a6") fid,
            in("a7") eid,
        );
    }
    (err, val)
}

#[allow(dead_code)]
pub fn sbi_set_timer(stime_value: u64) {
    sbi_call(0x00, 0, stime_value as usize, 0, 0);
}

pub fn sbi_shutdown() -> ! {
    sbi_call(0x53525354, 0, 0, 0, 0);
    sbi_call(0x08, 0, 0, 0, 0);
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}
