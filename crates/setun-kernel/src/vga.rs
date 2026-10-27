const PCI_ECAM_BASE: usize = 0x3000_0000;
const PCI_IO_BASE: usize = 0x0300_0000;
const FB_PHYS_ADDR: usize = 0x4000_0000;

const VBE_DISPI_IOPORT_INDEX: usize = 0x01CE;
const VBE_DISPI_IOPORT_DATA: usize = 0x01CF;

const VBE_DISPI_INDEX_ID: u16 = 0;
const VBE_DISPI_INDEX_XRES: u16 = 1;
const VBE_DISPI_INDEX_YRES: u16 = 2;
const VBE_DISPI_INDEX_BPP: u16 = 3;
const VBE_DISPI_INDEX_ENABLE: u16 = 4;

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
            // Assign BAR 0 (Framebuffer) to 0x4000_0000
            ((dev_addr + 0x10) as *mut u32).write_volatile(FB_PHYS_ADDR as u32);

            // Enable IO space, Memory space, and Bus Master in Command register (offset 0x04)
            let cmd = ((dev_addr + 0x04) as *const u16).read_volatile();
            ((dev_addr + 0x04) as *mut u16).write_volatile(cmd | 0x0007);
        }

        // Configure Bochs VBE
        Self::vbe_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_DISABLED);
        Self::vbe_write(VBE_DISPI_INDEX_XRES, width as u16);
        Self::vbe_write(VBE_DISPI_INDEX_YRES, height as u16);
        Self::vbe_write(VBE_DISPI_INDEX_BPP, 32);
        Self::vbe_write(VBE_DISPI_INDEX_ENABLE, VBE_DISPI_ENABLED | VBE_DISPI_LFB_ENABLED);

        let vbe_id = Self::vbe_read(VBE_DISPI_INDEX_ID);
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

    pub fn vbe_write(index: u16, val: u16) {
        unsafe {
            let index_port = (PCI_IO_BASE + VBE_DISPI_IOPORT_INDEX) as *mut u16;
            let data_port = (PCI_IO_BASE + VBE_DISPI_IOPORT_DATA) as *mut u16;
            index_port.write_volatile(index);
            data_port.write_volatile(val);
        }
    }

    pub fn vbe_read(index: u16) -> u16 {
        unsafe {
            let index_port = (PCI_IO_BASE + VBE_DISPI_IOPORT_INDEX) as *mut u16;
            let data_port = (PCI_IO_BASE + VBE_DISPI_IOPORT_DATA) as *mut u16;
            index_port.write_volatile(index);
            data_port.read_volatile()
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
                    self.fb_ptr.add(dst_row1 + dst_x).write_volatile(pixel);
                    self.fb_ptr.add(dst_row1 + dst_x + 1).write_volatile(pixel);
                }
            }
        }
    }
}
