use core::ptr::NonNull;
use virtio_drivers::device::input::VirtIOInput;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::transport::{DeviceType, Transport};
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

pub struct KernelHal;

unsafe impl Hal for KernelHal {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        let size = pages * 4096;
        let layout = core::alloc::Layout::from_size_align(size, 4096).unwrap();
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        let paddr = ptr as usize as u64;
        (paddr, NonNull::new(ptr).unwrap())
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, _vaddr: NonNull<u8>, pages: usize) -> i32 {
        let size = pages * 4096;
        let layout = core::alloc::Layout::from_size_align(size, 4096).unwrap();
        unsafe { alloc::alloc::dealloc((paddr as usize) as *mut u8, layout) };
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        NonNull::new((paddr as usize) as *mut u8).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        buffer.as_ptr() as *const u8 as usize as u64
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {}
}

pub struct MouseDriver {
    pub input: Option<VirtIOInput<KernelHal, MmioTransport<'static>>>,
}

impl MouseDriver {
    pub fn probe() -> Self {
        for slot in 1..=8 {
            let addr = 0x1000_0000 + slot * 0x1000;
            let header = NonNull::new(addr as *mut VirtIOHeader).unwrap();
            if let Ok(transport) = unsafe { MmioTransport::new(header, 0x1000) } {
                if transport.device_type() == DeviceType::Input {
                    if let Ok(input) = VirtIOInput::<KernelHal, MmioTransport<'static>>::new(transport) {
                        crate::kprintln!("   [OK] VirtIO Input Device initialized at MMIO 0x{:08x}", addr);
                        return Self { input: Some(input) };
                    }
                }
            }
        }
        crate::kprintln!("   [WARN] No VirtIO Input Device detected on MMIO slots");
        Self { input: None }
    }

    pub fn poll_events(&mut self, desktop: &mut setun_doom::SetunDesktop, dos: &mut setun_doom::SetunDos) {
        if let Some(ref mut input) = self.input {
            let _ = input.ack_interrupt();
            while let Some(event) = input.pop_pending_event() {
                match event.event_type {
                    3 => {
                        // EV_ABS: Absolute coordinates from VirtIO Tablet (0..32767)
                        if event.code == 0 {
                            desktop.mouse_x = (((event.value as i64) * 320) / 32768).clamp(0, 319);
                        } else if event.code == 1 {
                            desktop.mouse_y = (((event.value as i64) * 200) / 32768).clamp(0, 199);
                        }
                    }
                    2 => {
                        // EV_REL: Relative deltas from VirtIO Mouse
                        let delta = (event.value as i32) as i64;
                        if event.code == 0 {
                            desktop.mouse_x = (desktop.mouse_x + delta).clamp(0, 319);
                        } else if event.code == 1 {
                            desktop.mouse_y = (desktop.mouse_y + delta).clamp(0, 199);
                        }
                    }
                    1 => {
                        // EV_KEY: BTN_LEFT (0x110 = 272) or BTN_TOUCH (0x14a = 330)
                        if event.code == 0x110 || event.code == 0x14a {
                            if event.value == 1 {
                                crate::kprintln!(" * Mouse/Tablet Click at ({}, {})", desktop.mouse_x, desktop.mouse_y);
                                desktop.handle_mouse_click(dos, desktop.mouse_x, desktop.mouse_y);
                            } else {
                                desktop.mouse_down = false;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
