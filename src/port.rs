use core::fmt;

use crate::LineStsFlags;

/// A x86 I/O port-mapped UART.
#[cfg_attr(docsrs, doc(cfg(any(target_arch = "x86", target_arch = "x86_64"))))]
#[derive(Debug)]
pub struct SerialPort(u16 /* base port */);

impl SerialPort {
    /// Base port.
    fn port_base(&self) -> u16 {
        self.0
    }

    /// Data port.
    ///
    /// Read and write.
    fn port_data(&self) -> u16 {
        self.port_base()
    }

    /// Interrupt enable port.
    ///
    /// Write only.
    fn port_int_en(&self) -> u16 {
        self.port_base() + 1
    }

    /// Fifo control port.
    ///
    /// Write only.
    fn port_fifo_ctrl(&self) -> u16 {
        self.port_base() + 2
    }

    /// Line control port.
    ///
    /// Write only.
    fn port_line_ctrl(&self) -> u16 {
        self.port_base() + 3
    }

    /// Modem control port.
    ///
    /// Write only.
    fn port_modem_ctrl(&self) -> u16 {
        self.port_base() + 4
    }

    /// Line status port.
    ///
    /// Read only.
    fn port_line_sts(&self) -> u16 {
        self.port_base() + 5
    }

    /// Creates a new serial port interface on the given I/O base port.
    ///
    /// This function is unsafe because the caller must ensure that the given base address
    /// really points to a serial port device and that the caller has the necessary rights
    /// to perform the I/O operation.
    pub const unsafe fn new(base: u16) -> Self {
        Self(base)
    }

    /// Initializes the serial port.
    ///
    /// The default configuration of [38400/8-N-1](https://en.wikipedia.org/wiki/8-N-1) is used.
    pub fn init(&mut self) {
        unsafe {
            // Disable interrupts
            x86::io::outb(self.port_int_en(), 0x00);

            // Enable DLAB
            x86::io::outb(self.port_line_ctrl(), 0x80);

            // Set maximum speed to 38400 bps by configuring DLL and DLM
            x86::io::outb(self.port_data(), 0x03);
            x86::io::outb(self.port_int_en(), 0x00);

            // Disable DLAB and set data word length to 8 bits
            x86::io::outb(self.port_line_ctrl(), 0x03);

            // Enable FIFO, clear TX/RX queues and
            // set interrupt watermark at 14 bytes
            x86::io::outb(self.port_fifo_ctrl(), 0xc7);

            // Mark data terminal ready, signal request to send
            // and enable auxilliary output #2 (used as interrupt line for CPU)
            x86::io::outb(self.port_modem_ctrl(), 0x0b);

            // Enable interrupts
            x86::io::outb(self.port_int_en(), 0x01);
        }
    }

    fn line_sts(&mut self) -> LineStsFlags {
        unsafe { LineStsFlags::from_bits_truncate(x86::io::inb(self.port_line_sts())) }
    }

    /// Sends a byte on the serial port.
    pub fn send(&mut self, data: u8) {
        match data {
            8 | 0x7F => {
                self.send_raw(8);
                self.send_raw(b' ');
                self.send_raw(8);
            }
            data => {
                self.send_raw(data);
            }
        }
    }

    /// Sends a raw byte on the serial port, intended for binary data.
    pub fn send_raw(&mut self, data: u8) {
        unsafe {
            wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
            x86::io::outb(self.port_data(), data);
        }
    }

    /// Receives a byte on the serial port.
    pub fn receive(&mut self) -> u8 {
        unsafe {
            wait_for!(self.line_sts().contains(LineStsFlags::INPUT_FULL));
            x86::io::inb(self.port_data())
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.send(byte);
        }
        Ok(())
    }
}
