use core::fmt::{self, Write};

const UART_BASE: usize = 0x1000_0000;
const LSR_TX_EMPTY: u8 = 0x20;

pub struct Uart;

impl Uart {
    pub fn init() {
        unsafe {
            let ptr = UART_BASE as *mut u8;
            // Disable all interrupts
            ptr.add(1).write_volatile(0x00);
            // Enable FIFO, reset TX/RX queues
            ptr.add(2).write_volatile(0x07);
            // 8-bit word length, 1 stop bit, no parity
            ptr.add(3).write_volatile(0x03);
        }
    }

    pub fn putc(c: u8) {
        unsafe {
            let ptr = UART_BASE as *mut u8;
            while ptr.add(5).read_volatile() & LSR_TX_EMPTY == 0 {}
            ptr.add(0).write_volatile(c);
        }
    }

    pub fn getc() -> Option<u8> {
        unsafe {
            let ptr = UART_BASE as *mut u8;
            if ptr.add(5).read_volatile() & 0x01 != 0 {
                Some(ptr.add(0).read_volatile())
            } else {
                None
            }
        }
    }
}

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                Uart::putc(b'\r');
            }
            Uart::putc(b);
        }
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    let mut uart = Uart;
    let _ = uart.write_fmt(args);
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        $crate::uart::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! kprintln {
    () => {
        $crate::kprint!("\n")
    };
    ($($arg:tt)*) => {
        $crate::kprint!("{}\n", format_args!($($arg)*))
    };
}
