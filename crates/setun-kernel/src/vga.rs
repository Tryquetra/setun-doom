const PCI_ECAM_BASE: usize = 0x3000_0000;
const FB_PHYS_ADDR: usize = 0x4000_0000;
const MMIO_PHYS_ADDR: usize = 0x4100_0000;

// Offsets within VGA BAR 2 MMIO region
const PCI_VGA_IOPORT_OFFSET: usize = 0x400;
const PCI_VGA_BOCHS_OFFSET: usize = 0x500;

const VBE_DISPI_INDEX_ID: usize = 0;
const VBE_DISPI_INDEX_XRES: usize = 1;
const VBE_DISPI_INDEX_YRES: usize = 2;
const VBE_DISPI_INDEX_BPP: usize = 3;
const VBE_DISPI_INDEX_ENABLE: usize = 4;

const VBE_DISPI_DISABLED: u16 = 0x00;
const VBE_DISPI_ENABLED: u16 = 0x01;
const VBE_DISPI_LFB_ENABLED: u16 = 0x40;

pub struct BochsVga {
    pub fb_ptr: *mut u32,
    pub width: usize,
    pub height: usize,
}

impl BochsVga {
    pub fn probe_and_init(width: usize, height: usize) -> Option<Self> {
        let mut found_dev: Option<(usize, u16, u16)> = None;

        // Scan PCI bus 0 (device 0 to 31)
        for dev in 0..32 {
            let dev_addr = PCI_ECAM_BASE | (dev << 15);
            let id = unsafe { (dev_addr as *const u32).read_volatile() };
            if id == 0xFFFF_FFFF || id == 0 {
                continue;
            }
            let vendor_id = (id & 0xFFFF) as u16;
            let device_id = ((id >> 16) & 0xFFFF) as u16;

            // QEMU/Bochs standard VGA is 0x1234:0x1111
            if vendor_id == 0x1234 && device_id == 0x1111 {
                found_dev = Some((dev_addr, vendor_id, device_id));
                break;
            }

            // Check class code (0x03 = display controller)
            let class_rev = unsafe { ((dev_addr + 0x08) as *const u32).read_volatile() };
            let base_class = ((class_rev >> 24) & 0xFF) as u8;
            if base_class == 0x03 {
                found_dev = Some((dev_addr, vendor_id, device_id));
                break;
            }
        }

        let (dev_addr, vendor_id, device_id) = found_dev?;
        crate::kprintln!(
            "   [OK] Found PCI Display Adapter at 0x{:08x} (Vendor: 0x{:04x}, Device: 0x{:04x})",
            dev_addr,
            vendor_id,
            device_id
        );

        unsafe {
            // Assign BAR 0 (Framebuffer VRAM) to 0x4000_0000
            ((dev_addr + 0x10) as *mut u32).write_volatile(FB_PHYS_ADDR as u32);

            // Assign BAR 2 (MMIO registers) to 0x4100_0000
            ((dev_addr + 0x18) as *mut u32).write_volatile(MMIO_PHYS_ADDR as u32);

            // Enable IO space, Memory space, and Bus Master in Command register (offset 0x04)
            let cmd = ((dev_addr + 0x04) as *const u16).read_volatile();
            ((dev_addr + 0x04) as *mut u16).write_volatile(cmd | 0x0007);
        }

        // Unblank standard VGA display by toggling Attribute Controller PAS bit
        Self::unblank_vga();

        // Bochs Dispi registers mapped via BAR 2 at MMIO_PHYS_ADDR + 0x500
        let bochs_mmio = (MMIO_PHYS_ADDR + PCI_VGA_BOCHS_OFFSET) as *mut u16;

        let vbe_id_initial = unsafe { bochs_mmio.add(VBE_DISPI_INDEX_ID).read_volatile() };
        crate::kprintln!("   [OK] Bochs VBE initial hardware ID: 0x{:04x}", vbe_id_initial);

        // Configure resolution and color depth
        unsafe {
            bochs_mmio.add(VBE_DISPI_INDEX_ENABLE).write_volatile(VBE_DISPI_DISABLED);
            bochs_mmio.add(VBE_DISPI_INDEX_XRES).write_volatile(width as u16);
            bochs_mmio.add(VBE_DISPI_INDEX_YRES).write_volatile(height as u16);
            bochs_mmio.add(VBE_DISPI_INDEX_BPP).write_volatile(32);
            bochs_mmio.add(VBE_DISPI_INDEX_ENABLE).write_volatile(VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED);
        }

        // Unblank again to ensure Attribute Controller remains unblanked after VBE mode switch
        Self::unblank_vga();

        let vbe_id = unsafe { bochs_mmio.add(VBE_DISPI_INDEX_ID).read_volatile() };
        crate::kprintln!(
            "   [OK] Bochs VBE Display Mode Active: {}x{} @ 32bpp (VBE ID: 0x{:04x})",
            width,
            height,
            vbe_id
        );

        Some(Self {
            fb_ptr: FB_PHYS_ADDR as *mut u32,
            width,
            height,
        })
    }

    /// Unblanks the VGA screen by enabling the Attribute Controller PAS bit (bit 5).
    pub fn unblank_vga() {
        unsafe {
            let vga_io = (MMIO_PHYS_ADDR + PCI_VGA_IOPORT_OFFSET) as *mut u8;
            // 1. Reset attribute controller flip-flop by reading input status register 1 (0x3DA -> offset 0x1A)
            let _ = vga_io.add(0x1A).read_volatile();
            // 2. Write 0x20 to Attribute Controller Address Register (0x3C0 -> offset 0x00)
            // Bit 5 (0x20) unblanks the display and enables palette address sourcing
            vga_io.add(0x00).write_volatile(0x20);
        }
    }

    /// Blits 320x200 Setun Memory framebuffer scaled 2x2 to a 640x400 32-bit linear screen.
    pub fn blit_setun_framebuffer_2x(&self, mem: &setun_doom::Memory) {
        let fb_slice = mem.framebuffer_slice();
        for y in 0..200 {
            let src_row = y * 320;
            let dst_row0 = (y * 2) * self.width;
            let dst_row1 = (y * 2 + 1) * self.width;
            for x in 0..320 {
                let tryte = setun_doom::Tryte6::from_i64(fb_slice[src_row + x]);
                let pal_idx = tryte.to_palette_index() as usize;
                let [r, g, b, _] = mem.palette_rgba[pal_idx];
                let pixel: u32 = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                let dst_x = x * 2;
                unsafe {
                    self.fb_ptr.add(dst_row0 + dst_x).write_volatile(pixel);
                    self.fb_ptr.add(dst_row0 + dst_x + 1).write_volatile(pixel);
                    self.fb_ptr.add(dst_row1 + dst_x + 1).write_volatile(pixel);
                    self.fb_ptr.add(dst_row1 + dst_x + 1).write_volatile(pixel);
                }
            }
        }
    }
}
