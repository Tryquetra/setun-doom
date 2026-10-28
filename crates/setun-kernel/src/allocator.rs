use buddy_system_allocator::LockedHeap;

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

/// 128 MB heap allocation pool
pub const HEAP_SIZE: usize = 128 * 1024 * 1024;

pub fn init_heap() -> (usize, usize) {
    extern "C" {
        static _ekernel: u8;
    }
    let heap_start = unsafe { ((&_ekernel as *const u8 as usize) + 0xfff) & !0xfff };
    unsafe {
        HEAP_ALLOCATOR.lock().init(heap_start, HEAP_SIZE);
    }
    (heap_start, heap_start + HEAP_SIZE)
}
