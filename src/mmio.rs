use core::{
    fmt,
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{LineStsFlags, WouldBlockError};

/// A memory-mapped UART.
#[derive(Debug)]
pub struct MmioSerialPort {
    data: AtomicPtr<u8>,
    int_en: AtomicPtr<u8>,
    fifo_ctrl: AtomicPtr<u8>,
    line_ctrl: AtomicPtr<u8>,
    modem_ctrl: AtomicPtr<u8>,
    line_sts: AtomicPtr<u8>,
}

impl MmioSerialPort {
    /// Creates a new UART interface on the given memory mapped address.
    ///
    /// This function is unsafe because the caller must ensure that the given base address
    /// really points to a serial port device.
    #[rustversion::attr(since(1.61), const)]
    pub unsafe fn new(base: usize) -> Self {
        Self::new_with_stride(base, 1)
    }

    /// Creates a new UART interface on the given memory mapped address with a given
    /// register stride.
    ///
    /// This function is unsafe because the caller must ensure that the given base address
    /// really points to a serial port device.
    #[rustversion::attr(since(1.61), const)]
    pub unsafe fn new_with_stride(base: usize, stride: usize) -> Self {
        let base_pointer = base as *mut u8;
        Self {
            data: AtomicPtr::new(base_pointer),
            int_en: AtomicPtr::new(base_pointer.add(1 * stride)),
            fifo_ctrl: AtomicPtr::new(base_pointer.add(2 * stride)),
            line_ctrl: AtomicPtr::new(base_pointer.add(3 * stride)),
            modem_ctrl: AtomicPtr::new(base_pointer.add(4 * stride)),
            line_sts: AtomicPtr::new(base_pointer.add(5 * stride)),
        }
    }

    /// Initializes the memory-mapped UART.
    ///
    /// The default configuration of [38400/8-N-1](https://en.wikipedia.org/wiki/8-N-1) is used.
    pub fn init(&mut self) {
        let self_int_en = self.int_en.load(Ordering::Relaxed);
        let self_line_ctrl = self.line_ctrl.load(Ordering::Relaxed);
        let self_data = self.data.load(Ordering::Relaxed);
        let self_fifo_ctrl = self.fifo_ctrl.load(Ordering::Relaxed);
        let self_modem_ctrl = self.modem_ctrl.load(Ordering::Relaxed);
        unsafe {
            // Disable interrupts
            self_int_en.write(0x00);

            // Enable DLAB
            self_line_ctrl.write(0x80);

            // Set maximum speed to 38400 bps by configuring DLL and DLM
            self_data.write(0x03);
            self_int_en.write(0x00);

            // Disable DLAB and set data word length to 8 bits
            self_line_ctrl.write(0x03);

            // Enable FIFO, clear TX/RX queues and
            // set interrupt watermark at 14 bytes
            self_fifo_ctrl.write(0xC7);

            // Mark data terminal ready, signal request to send
            // and enable auxilliary output #2 (used as interrupt line for CPU)
            self_modem_ctrl.write(0x0B);

            // Enable interrupts
            self_int_en.write(0x01);
        }
    }

    fn line_sts(&mut self) -> LineStsFlags {
        unsafe { LineStsFlags::from_bits_truncate(*self.line_sts.load(Ordering::Relaxed)) }
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
        retry_until_ok!(self.try_send_raw(data))
    }

    /// Tries to send a raw byte on the serial port, intended for binary data.
    pub fn try_send_raw(&mut self, data: u8) -> Result<(), WouldBlockError> {
        if self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY) {
            let self_data = self.data.load(Ordering::Relaxed);
            unsafe {
                self_data.write(data);
            }
            Ok(())
        } else {
            Err(WouldBlockError)
        }
    }

    /// Receives a byte on the serial port.
    pub fn receive(&mut self) -> u8 {
        retry_until_ok!(self.try_receive())
    }

    /// Tries to receive a byte on the serial port.
    pub fn try_receive(&mut self) -> Result<u8, WouldBlockError> {
        if self.line_sts().contains(LineStsFlags::INPUT_FULL) {
            let self_data = self.data.load(Ordering::Relaxed);
            let data = unsafe { self_data.read() };
            Ok(data)
        } else {
            Err(WouldBlockError)
        }
    }
}

impl fmt::Write for MmioSerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.send(byte);
        }
        Ok(())
    }
}
