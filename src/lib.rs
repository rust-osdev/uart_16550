//! Minimal support for uart_16550 serial I/O.
//!
//! # Usage
//! ## With `port_{stable, nightly}` feature
//!
//! ```rust
//! use uart_16550::SerialPort;
//!
//! const SERIAL_IO_PORT: u16 = 0x3F8;
//!
//! let mut serial_port = unsafe { SerialPort::new(SERIAL_IO_PORT) };
//! serial_port.init();
//!
//! // Now the serial port is ready to be used. To send a byte:
//! serial_port.send(42);
//!
//! // To receive a byte:
//! let data = serial_port.receive();
//! ```
//!
//! ## With `mmio_{stable, nightly}` feature
//!
//! ```rust
//! use uart_16550::SerialPort;
//!
//! const SERIAL_IO_PORT: usize = 0x1000_0000;
//!
//! let mut serial_port = unsafe { SerialPort::new(SERIAL_IO_PORT) };
//! serial_port.init();
//!
//! // Now the serial port is ready to be used. To send a byte:
//! serial_port.send(42);
//!
//! // To receive a byte:
//! let data = serial_port.receive();
//! ```

#![no_std]
#![warn(missing_docs)]
#![cfg_attr(feature = "mmio_nightly", feature(const_ptr_offset))]

use bitflags::bitflags;
use core::fmt;

#[cfg(any(
    all(
        not(any(feature = "port_stable", feature = "port_nightly")),
        not(any(feature = "mmio_stable", feature = "mmio_nightly"))
    ),
    all(
        any(feature = "port_stable", feature = "port_nightly"),
        any(feature = "mmio_stable", feature = "mmio_nightly")
    )
))]
compile_error!(
    "One of these features must be enabled: `port_{stable, nightly}`, `mmio_{stable, nightly}`"
);

#[cfg(any(feature = "port_stable", feature = "port_nightly"))]
use x86_64::instructions::port::Port;

#[cfg(any(feature = "mmio_stable", feature = "mmio_nightly"))]
use core::sync::atomic::{AtomicPtr, Ordering};

macro_rules! wait_for {
    ($cond:expr) => {
        while !$cond {
            core::hint::spin_loop()
        }
    };
}

bitflags! {
    /// Interrupt enable flags
    struct IntEnFlags: u8 {
        const RECEIVED = 1;
        const SENT = 1 << 1;
        const ERRORED = 1 << 2;
        const STATUS_CHANGE = 1 << 3;
        // 4 to 7 are unused
    }
}

bitflags! {
    /// Line status flags
    struct LineStsFlags: u8 {
        const INPUT_FULL = 1;
        // 1 to 4 unknown
        const OUTPUT_EMPTY = 1 << 5;
        // 6 and 7 unknown
    }
}

#[cfg(any(feature = "port_stable", feature = "port_nightly"))]
/// An interface to a serial port that allows sending out individual bytes.
pub struct SerialPort {
    data: Port<u8>,
    int_en: Port<u8>,
    fifo_ctrl: Port<u8>,
    line_ctrl: Port<u8>,
    modem_ctrl: Port<u8>,
    line_sts: Port<u8>,
}

#[cfg(any(feature = "port_stable", feature = "port_nightly"))]
impl SerialPort {
    /// Creates a new serial port interface on the given I/O port.
    ///
    /// This function is unsafe because the caller must ensure that the given base address
    /// really points to a serial port device.
    #[cfg(feature = "port_nightly")]
    pub const unsafe fn new(base: u16) -> Self {
        Self {
            data: Port::new(base),
            int_en: Port::new(base + 1),
            fifo_ctrl: Port::new(base + 2),
            line_ctrl: Port::new(base + 3),
            modem_ctrl: Port::new(base + 4),
            line_sts: Port::new(base + 5),
        }
    }

    /// Creates a new serial port interface on the given I/O port.
    ///
    /// This function is unsafe because the caller must ensure that the given base address
    /// really points to a serial port device.
    #[cfg(feature = "port_stable")]
    pub unsafe fn new(base: u16) -> Self {
        Self {
            data: Port::new(base),
            int_en: Port::new(base + 1),
            fifo_ctrl: Port::new(base + 2),
            line_ctrl: Port::new(base + 3),
            modem_ctrl: Port::new(base + 4),
            line_sts: Port::new(base + 5),
        }
    }

    /// Initializes the serial port.
    ///
    /// The default configuration of [38400/8-N-1](https://en.wikipedia.org/wiki/8-N-1) is used.
    pub fn init(&mut self) {
        unsafe {
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
    }

    fn line_sts(&mut self) -> LineStsFlags {
        unsafe { LineStsFlags::from_bits_truncate(self.line_sts.read()) }
    }

    /// Sends a byte on the serial port.
    pub fn send(&mut self, data: u8) {
        unsafe {
            match data {
                8 | 0x7F => {
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self.data.write(8);
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self.data.write(b' ');
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self.data.write(8)
                }
                _ => {
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self.data.write(data);
                }
            }
        }
    }

    /// Receives a byte on the serial port.
    pub fn receive(&mut self) -> u8 {
        unsafe {
            wait_for!(self.line_sts().contains(LineStsFlags::INPUT_FULL));
            self.data.read()
        }
    }
}

/// An interface to a serial port that allows sending out individual bytes.
#[cfg(any(feature = "mmio_stable", feature = "mmio_nightly"))]
pub struct SerialPort {
    data: AtomicPtr<u8>,
    int_en: AtomicPtr<u8>,
    fifo_ctrl: AtomicPtr<u8>,
    line_ctrl: AtomicPtr<u8>,
    modem_ctrl: AtomicPtr<u8>,
    line_sts: AtomicPtr<u8>,
}

#[cfg(any(feature = "mmio_stable", feature = "mmio_nightly"))]
impl SerialPort {
    /// Creates a new serial port interface on the given memory mapped address.
    ///
    /// This function is unsafe because the caller must ensure that the given base address
    /// really points to a serial port device.
    #[cfg(feature = "mmio_nightly")]
    pub const unsafe fn new(base: usize) -> Self {
        let base_pointer = base as *mut u8;
        Self {
            data: AtomicPtr::new(base_pointer),
            int_en: AtomicPtr::new(base_pointer.add(1)),
            fifo_ctrl: AtomicPtr::new(base_pointer.add(2)),
            line_ctrl: AtomicPtr::new(base_pointer.add(3)),
            modem_ctrl: AtomicPtr::new(base_pointer.add(4)),
            line_sts: AtomicPtr::new(base_pointer.add(5)),
        }
    }

    #[cfg(feature = "mmio_stable")]
    pub unsafe fn new(base: usize) -> Self {
        let base_pointer = base as *mut u8;
        Self {
            data: AtomicPtr::new(base_pointer),
            int_en: AtomicPtr::new(base_pointer.add(1)),
            fifo_ctrl: AtomicPtr::new(base_pointer.add(2)),
            line_ctrl: AtomicPtr::new(base_pointer.add(3)),
            modem_ctrl: AtomicPtr::new(base_pointer.add(4)),
            line_sts: AtomicPtr::new(base_pointer.add(5)),
        }
    }

    /// Initializes the serial port.
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
        let self_data = self.data.load(Ordering::Relaxed);
        unsafe {
            match data {
                8 | 0x7F => {
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self_data.write(8);
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self_data.write(b' ');
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self_data.write(8)
                }
                _ => {
                    wait_for!(self.line_sts().contains(LineStsFlags::OUTPUT_EMPTY));
                    self_data.write(data);
                }
            }
        }
    }

    /// Receives a byte on the serial port.
    pub fn receive(&mut self) -> u8 {
        let self_data = self.data.load(Ordering::Relaxed);
        unsafe {
            wait_for!(self.line_sts().contains(LineStsFlags::INPUT_FULL));
            self_data.read()
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
