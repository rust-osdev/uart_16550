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
//! use uart_16550::MmioSerialPort;
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
#![cfg_attr(feature = "nightly", feature(const_ptr_offset))]

use bitflags::bitflags;

#[cfg(not(any(feature = "stable", feature = "nightly")))]
compile_error!("Either the `stable` or `nightly` feature must be enabled");

macro_rules! wait_for {
    ($cond:expr) => {
        while !$cond {
            core::hint::spin_loop()
        }
    };
}

pub mod mmio;
#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use crate::x86_64::SerialPort;
pub use crate::mmio::MmioSerialPort;

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
