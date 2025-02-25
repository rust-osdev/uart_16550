use core::fmt;

use crate::{register::Uart16550Register, LineStsFlags, WouldBlockError};

/// Trait for using a 16550 compatible interface regardless of how it's connected
pub trait Uart16550: fmt::Write {
    /// Initializes the UART.
    ///
    /// The default configuration of [38400/8-N-1](https://en.wikipedia.org/wiki/8-N-1) is used.
    fn init(&mut self);

    /// Sends a byte on the serial port.
    fn send(&mut self, data: u8);

    /// Sends a raw byte, intended for binary data.
    fn send_raw(&mut self, data: u8) {
        retry_until_ok!(self.try_send_raw(data))
    }

    /// Tries to send a raw byte, intended for binary data.
    fn try_send_raw(&mut self, data: u8) -> Result<(), WouldBlockError>;

    /// Receives a byte.
    fn receive(&mut self) -> u8 {
        retry_until_ok!(self.try_receive())
    }

    /// Tries to receive a byte.
    fn try_receive(&mut self) -> Result<u8, WouldBlockError>;
}

/// A struct with all the 16550 registers needed to send and receive data
pub struct Uart16550Registers<R>
where
    R: Uart16550Register,
{
    pub(crate) data: R,
    pub(crate) int_en: R,
    pub(crate) fifo_ctrl: R,
    pub(crate) line_ctrl: R,
    pub(crate) modem_ctrl: R,
    pub(crate) line_sts: R,
}

impl<R: Uart16550Register> Uart16550Registers<R> {
    fn line_sts(&mut self) -> LineStsFlags {
        LineStsFlags::from_bits_truncate(self.line_sts.read())
    }
}

impl<R: Uart16550Register> Uart16550 for Uart16550Registers<R> {
    fn init(&mut self) {
        // Disable interrupts
        self.int_en.write(0x00);

        // Enable DLAB
        self.line_ctrl.write(0x80);

        // Set maximum speed to 38400 bps by configuring DLL and DLM
        self.data.write(0x03);
        self.int_en.write(0x00);

        // Disable DLAB and set data word length to 8 bits
        self.line_ctrl.write(0x03);

        // Enable FIFO, clear TX/RX queues and
        // set interrupt watermark at 14 bytes
        self.fifo_ctrl.write(0xC7);

        // Mark data terminal ready, signal request to send
        // and enable auxilliary output #2 (used as interrupt line for CPU)
        self.modem_ctrl.write(0x0B);

        // Enable interrupts
        self.int_en.write(0x01);
    }

    fn send(&mut self, data: u8) {
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

    fn try_send_raw(&mut self, data: u8) -> Result<(), WouldBlockError> {
        if self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY) {
            self.data.write(data);
            Ok(())
        } else {
            Err(WouldBlockError)
        }
    }

    fn try_receive(&mut self) -> Result<u8, WouldBlockError> {
        if self.line_sts().contains(LineStsFlags::INPUT_FULL) {
            let data = self.data.read();
            Ok(data)
        } else {
            Err(WouldBlockError)
        }
    }
}

impl<R: Uart16550Register> fmt::Write for Uart16550Registers<R> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.send(byte);
        }
        Ok(())
    }
}
