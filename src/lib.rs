//! Minimal support for
//! [serial communication](https://en.wikipedia.org/wiki/Asynchronous_serial_communication)
//! through [UART](https://en.wikipedia.org/wiki/Universal_asynchronous_receiver-transmitter)
//! devices, which are compatible to the [16550 UART](https://en.wikipedia.org/wiki/16550_UART).
//!
//! This crate supports I/O port-mapped (x86 only) and memory-mapped UARTS.
//!
//! ## Usage
//!
//! Depending on the system architecture, the UART can be either accessed through
//! [port-mapped I/O](https://wiki.osdev.org/Port_IO) or
//! [memory-mapped I/O](https://en.wikipedia.org/wiki/Memory-mapped_I/O).
//!
//! ### With port-mappd I/O
//!
//! The UART is accessed through port-mapped I/O on architectures such as `x86_64`.
//! On these architectures, the  [`SerialPort`] type can be used:
//!
//!
//! ```no_run
//! # #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
//! # fn main() {
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
//! # }
//! # #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
//! # fn main() {}
//! ```
//!
//! ### With memory mapped serial port
//!
//! Most other architectures, such as [RISC-V](https://en.wikipedia.org/wiki/RISC-V), use
//! memory-mapped I/O for accessing the UARTs. On these architectures, the [`MmioSerialPort`]
//! type can be used:
//!
//! ```no_run
//! use uart_16550::MmioSerialPort;
//!
//! const SERIAL_PORT_BASE_ADDRESS: usize = 0x1000_0000;
//!
//! let mut serial_port = unsafe { MmioSerialPort::new(SERIAL_PORT_BASE_ADDRESS) };
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
#![cfg_attr(docsrs, feature(doc_cfg))]

use bitflags::bitflags;

macro_rules! wait_for {
    ($cond:expr) => {
        while !$cond {
            core::hint::spin_loop()
        }
    };
}

/// Memory mapped implementation
mod mmio;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
/// Port asm commands implementation
mod port;

pub use crate::mmio::MmioSerialPort;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use crate::port::SerialPort;

bitflags! {
    /// Interrupt enable flags
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct LineStsFlags: u8 {
        const INPUT_FULL = 1;
        // 1 to 4 unknown
        const OUTPUT_EMPTY = 1 << 5;
        // 6 and 7 unknown
    }
}
