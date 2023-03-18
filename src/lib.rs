//! Minimal support for
//! [serial communication](https://en.wikipedia.org/wiki/Asynchronous_serial_communication)
//! through [UART](https://en.wikipedia.org/wiki/Universal_asynchronous_receiver-transmitter)
//! devices, which are compatible to the [16550 UART](https://en.wikipedia.org/wiki/16550_UART).
//!
//! This crate supports port-mapped and memory mapped UARTS.
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
//! # #[cfg(target_arch = "x86_64")]
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
//! # #[cfg(not(target_arch = "x86_64"))]
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
#[cfg(target_arch = "x86_64")]
/// Port asm commands implementation
mod port;

pub use crate::mmio::MmioSerialPort;
#[cfg(target_arch = "x86_64")]
pub use crate::port::SerialPort;

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
        // values from https://www.lammertbies.nl/comm/info/serial-uart
        const INPUT_FULL = 1;
        const OVERRUN_ERROR = 1 << 1;
        const PARITY_ERROR = 1 << 2;
        const FRAMING_ERROR = 1 << 3;
        const BREAK_RECEIVED = 1 << 4;
        const OUTPUT_EMPTY = 1 << 5;
        const OUTPUT_EMPTY_IDLE = 1 << 6;
        const ERROR_DATA_FIFO = 1 << 7;
    }
}

/// Divisor latch register values used for setting the baud rate
///
/// See: https://www.lammertbies.nl/comm/info/serial-uart#DLX
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DivisorLatchFlags {
    dll: DivisorLatchLeastFlags,
    dlm: DivisorLatchMostFlags,
}

impl DivisorLatchFlags {
    /// Create a new DivisorLatchFlags structure with provided DLL and DLM values
    /// See [DivisorLatchLeastFlags] and [DivisorLatchMostFlags] for sane combinations
    #[rustversion::attr(since(1.61), const)]
    pub fn new(dll: DivisorLatchLeastFlags, dlm: DivisorLatchMostFlags) -> Self {
        Self { dll, dlm }
    }

    /// Create a Divisor Latch setting for 300 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_50() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_50, dlm: DivisorLatchMostFlags::BAUD_50 }
    }

    /// Create a Divisor Latch setting for 300 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_300() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_300, dlm: DivisorLatchMostFlags::BAUD_300 }
    }

    /// Create a Divisor Latch setting for 1,200 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_1200() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_1200, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Create a Divisor Latch setting for 2,400 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_2400() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_2400, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Create a Divisor Latch setting for 4,800 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_4800() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_4800, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Create a Divisor Latch setting for 9,600 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_9600() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_9600, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Create a Divisor Latch setting for 19,200 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_19200() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_19200, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Create a Divisor Latch setting for 38,400 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_38400() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_38400, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Create a Divisor Latch setting for 57,600 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_57600() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_57600, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Create a Divisor Latch setting for 115,200 baud rate
    #[rustversion::attr(since(1.61), const)]
    pub fn baud_115200() -> Self {
        Self { dll: DivisorLatchLeastFlags::BAUD_115200, dlm: DivisorLatchMostFlags::BAUD_1200_115200 }
    }

    /// Consume the DivisorLatchFlags, and split into least- and most-significant flags
    #[rustversion::attr(since(1.61), const)]
    pub fn split(self) -> (DivisorLatchLeastFlags, DivisorLatchMostFlags) {
        (self.dll, self.dlm)
    }

    /// Get the DLL value
    #[rustversion::attr(since(1.61), const)]
    pub fn dll(&self) -> DivisorLatchLeastFlags {
        self.dll
    }

    /// Get the DLM value
    #[rustversion::attr(since(1.61), const)]
    pub fn dlm(&self) -> DivisorLatchMostFlags {
        self.dlm
    }

    /// Validate settings by checking for sane combinations of DLL and DLM values
    #[rustversion::attr(since(1.61), const)]
    pub fn validate(&self) -> bool {
        match (self.dll, self.dlm) {
            (DivisorLatchLeastFlags::BAUD_50, DivisorLatchMostFlags::BAUD_50) |
            (DivisorLatchLeastFlags::BAUD_300, DivisorLatchMostFlags::BAUD_300) |
            (DivisorLatchLeastFlags::BAUD_1200, DivisorLatchMostFlags::BAUD_1200_115200) |
            (DivisorLatchLeastFlags::BAUD_2400, DivisorLatchMostFlags::BAUD_1200_115200) |
            (DivisorLatchLeastFlags::BAUD_4800, DivisorLatchMostFlags::BAUD_1200_115200) |
            (DivisorLatchLeastFlags::BAUD_9600, DivisorLatchMostFlags::BAUD_1200_115200) |
            (DivisorLatchLeastFlags::BAUD_19200, DivisorLatchMostFlags::BAUD_1200_115200) |
            (DivisorLatchLeastFlags::BAUD_38400, DivisorLatchMostFlags::BAUD_1200_115200) |
            (DivisorLatchLeastFlags::BAUD_57600, DivisorLatchMostFlags::BAUD_1200_115200) |
            (DivisorLatchLeastFlags::BAUD_115200, DivisorLatchMostFlags::BAUD_1200_115200) => true,
            _ => false,
        }
    }
}

bitflags! {
    /// Divisor latch least-significant flags
    ///
    /// See: https://www.lammertbies.nl/comm/info/serial-uart#DLX
    pub struct DivisorLatchLeastFlags: u8 {
        /// Requires DLM set to 0x09
        const BAUD_50 = 0x00;
        /// Requires DLM set to 0x01
        const BAUD_300 = 0x80;
        /// Requires DLM set to 0x00
        const BAUD_1200 = 0x60;
        /// Requires DLM set to 0x00
        const BAUD_2400 = 0x30;
        /// Requires DLM set to 0x00
        const BAUD_4800 = 0x18;
        /// Requires DLM set to 0x00
        const BAUD_9600 = 0x0C;
        /// Requires DLM set to 0x00
        const BAUD_19200 = 0x06;
        /// Requires DLM set to 0x00
        const BAUD_38400 = 0x03;
        /// Requires DLM set to 0x00
        const BAUD_57600 = 0x02;
        /// Requires DLM set to 0x00
        const BAUD_115200 = 0x01;
    }
}

bitflags! {
    /// Divisor latch most-significant flags
    ///
    /// See: https://www.lammertbies.nl/comm/info/serial-uart#DLX
    pub struct DivisorLatchMostFlags: u8 {
        /// Requires DLL set to 0x00
        const BAUD_50 = 0x09;
        /// Requires DLL set to 0x80
        const BAUD_300 = 0x01;
        /// All rates from 1,200 to 115,200 require DLM set to 0x00
        const BAUD_1200_115200 = 0x00;
    }
}

bitflags! {
    /// FIFO control register
    ///
    /// See: https://www.lammertbies.nl/comm/info/serial-uart#FCR
    pub struct FifoCtrlFlags: u8 {
        /// Disable FIFO field
        const DISABLE        = 0b0000_0000;
        /// Enable FIFO field
        const ENABLE         = 0b0000_0001;
        /// Clear receive field
        const CLEAR_RECEIVE  = 0b0000_0010;
        /// Clear transmit field
        const CLEAR_TRANSMIT = 0b0000_0100;
        /// Set DMA mode 1
        const DMA_MODE1      = 0b0000_1000;
        /// Enable 64 byte FIFO (16750)
        const ENABLE_64B     = 0b0010_0000;
        /// 4 byte FIFO
        const BYTELEN_4      = 0b0100_0000;
        /// 8 byte FIFO
        const BYTELEN_8      = 0b1000_0000;
        /// 16 byte FIFO
        const BYTELEN_16     = 0b1100_0000;
    }
}

bitflags! {
    /// Modem control register
    ///
    /// See: https://www.lammertbies.nl/comm/info/serial-uart#MSR
    pub struct ModemCtrlFlags: u8 {
        /// Data terminal ready
        const DATA_TERMINAL_READY  = 0b0000_0001;
        /// Request to send
        const REQUEST_TO_SEND      = 0b0000_0010;
        /// Auxilliary output 1
        const AUX_OUTPUT1          = 0b0000_0100;
        /// Auxilliary output 2
        const AUX_OUTPUT2          = 0b0000_1000;
        /// Loopback mode
        const LOOPBACK             = 0b0001_0000;
    }
}

bitflags! {
    /// Modem status register
    ///
    /// See: https://www.lammertbies.nl/comm/info/serial-uart#MSR
    pub struct ModemStsFlags: u8 {
        /// Change in clear to send field
        const CHANGE_CLEAR_TO_SEND  = 0b0000_0001;
        /// Change in data set ready field
        const CHANGE_DATA_SET_READY = 0b0000_0010;
        /// Change in trailing edge ring indicator field
        const CHANGE_RING_INDICATOR = 0b0000_0100;
        /// Change in carrier detect field
        const CHANGE_CARRIER_DETECT = 0b0000_1000;
        /// Clear to send field
        const CLEAR_TO_SEND         = 0b0001_0000;
        /// Data set ready field
        const DATA_SET_READY        = 0b0010_0000;
        /// Ring indicator field
        const RING_INDICATOR        = 0b0100_0000;
        /// Carrier detect field
        const CARRIER_DETECT        = 0b1000_0000;
    }
}
