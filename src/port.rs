use crate::{register::Uart16550Register, uart_16550::Uart16550Registers};

/// A x86 I/O port-mapped 16550 register
pub struct PortAccessedRegister {
    port: u16,
}

impl PortAccessedRegister {
    const unsafe fn new(port: u16) -> Self {
        Self { port }
    }
}

impl Uart16550Register for PortAccessedRegister {
    fn read(&self) -> u8 {
        unsafe { x86::io::inb(self.port) }
    }

    fn write(&mut self, value: u8) {
        unsafe {
            x86::io::outb(self.port, value);
        }
    }
}

/// Creates a new serial port interface on the given I/O base port.
///
/// # Safety
/// This function is unsafe because the caller must ensure that the given base address
/// really points to a serial port device and that the caller has the necessary rights
/// to perform the I/O operation.
pub const unsafe fn new(base: u16) -> Uart16550Registers<PortAccessedRegister> {
    Uart16550Registers {
        data: PortAccessedRegister::new(base),
        int_en: PortAccessedRegister::new(base + 1),
        fifo_ctrl: PortAccessedRegister::new(base + 2),
        line_ctrl: PortAccessedRegister::new(base + 3),
        modem_ctrl: PortAccessedRegister::new(base + 4),
        line_sts: PortAccessedRegister::new(base + 5),
    }
}
