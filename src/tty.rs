// SPDX-License-Identifier: MIT OR Apache-2.0

//! Provides a thin abstraction over a [`Uart16550`] for VT102-like terminal
//! emulators on the receiving side.
//!
//! This module is suited for basic use cases and toy projects, but full VT102
//! compatibility is explicitly not a goal.
//!
//! For lower-level access of the underlying hardware, use [`Uart16550`]
//! instead.
//!
//! See [`Uart16550Tty`].

use crate::backend::{Backend, MmioAddress, MmioBackend, RegisterAddress};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::{PioBackend, PortIoAddress};
use crate::{Config, InitError, InvalidAddressError, LoopbackError, Uart16550};
use core::error::Error;
use core::fmt::{self, Display, Formatter};

/// Errors that [`Uart16550Tty::new_port`] and [`Uart16550Tty::new_mmio`] may
/// return.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Uart16550TtyError<A: RegisterAddress> {
    /// The underlying address is invalid.
    AddressError(InvalidAddressError<A>),
    /// Error initializing the device.
    InitError(InitError),
    /// The device could not be tested for proper operation.
    TestError(LoopbackError),
}

impl<A: RegisterAddress> Display for Uart16550TtyError<A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressError(e) => {
                write!(f, "{e}")
            }
            Self::InitError(e) => {
                write!(f, "error initializing the device: {e}")
            }
            Self::TestError(e) => {
                write!(f, "error testing the device: {e}")
            }
        }
    }
}

impl<A: RegisterAddress + 'static> Error for Uart16550TtyError<A> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AddressError(e) => Some(e),
            Self::InitError(e) => Some(e),
            Self::TestError(e) => Some(e),
        }
    }
}

/// Thin opinionated abstraction over [`Uart16550`] that helps to send Rust
/// strings easily to the other side, assuming the remote is a TTY (terminal).
///
/// It is especially suited as very easy way to see something when you develop
/// and test things in a VM.
///
/// It implements [`fmt::Write`].
///
/// # Example
/// ```rust,no_run
/// use uart_16550::{Config, Uart16550Tty};
/// use core::fmt::Write;
///
/// // SAFETY: The address is valid and we have exclusive access.
/// let mut uart = unsafe { Uart16550Tty::new_mmio(0x1000 as *mut _, Config::default()).expect("should initialize device") };
/// //                                    ^ or `new_port(0x3f8)`
/// uart.write_str("hello world\nhow's it going?");
/// ```
///
/// # MMIO and Port I/O
///
/// Uart 16550 devices are typically mapped via port I/O on x86 and via MMIO on
/// other platforms. The constructors `new_port()` and `new_mmio()` create an
/// instance of a device with the corresponding backend.
///
/// # Hints for Usage on Real Hardware
///
/// Please note that real hardware often behaves quite differently. Just because
/// this works in a VM (e.g., Cloud Hypervisor and VMM), your application
/// doesn't necessarily work on real hardware. You might need to fiddle with
/// the configuration or perform more research for potential hardware quirks
/// of your system.
#[derive(Debug)]
pub struct Uart16550Tty<B: Backend>(Uart16550<B>);

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Uart16550Tty<PioBackend> {
    /// Creates a new [`Uart16550Tty`] backed by x86 port I/O.
    ///
    /// Initializes the device and performs a self-test.
    ///
    /// # Safety
    ///
    /// Callers must ensure that the base port is valid and safe to use for the
    /// **whole lifetime** of the device. Further, all [`NUM_REGISTERS`]
    /// registers must be safely reachable from the base address.
    ///
    /// [`NUM_REGISTERS`]: crate::spec::NUM_REGISTERS
    pub unsafe fn new_port(
        base_port: u16,
        config: Config,
    ) -> Result<Self, Uart16550TtyError<PortIoAddress>> {
        // SAFETY: The address is valid and we have exclusive access.
        let mut inner =
            unsafe { Uart16550::new_port(base_port).map_err(Uart16550TtyError::AddressError)? };
        inner.init(config).map_err(Uart16550TtyError::InitError)?;
        inner
            .test_loopback()
            .map_err(Uart16550TtyError::TestError)?;
        Ok(Self(inner))
    }
}

impl Uart16550Tty<MmioBackend> {
    /// Creates a new [`Uart16550Tty`] backed by MMIO.
    ///
    /// Initializes the device and performs a self-test.
    ///
    /// # Safety
    ///
    /// Callers must ensure that the base address is valid and safe to use for
    /// the **whole lifetime** of the device. Further, all [`NUM_REGISTERS`]
    /// registers must be safely reachable from the base address.
    ///
    /// [`NUM_REGISTERS`]: crate::spec::NUM_REGISTERS
    pub unsafe fn new_mmio(
        base_address: *mut u8,
        config: Config,
    ) -> Result<Self, Uart16550TtyError<MmioAddress>> {
        // SAFETY: The address is valid and we have exclusive access.
        let mut inner =
            unsafe { Uart16550::new_mmio(base_address).map_err(Uart16550TtyError::AddressError)? };

        inner.init(config).map_err(Uart16550TtyError::InitError)?;
        inner
            .test_loopback()
            .map_err(Uart16550TtyError::TestError)?;
        Ok(Self(inner))
    }
}

impl<B: Backend> Uart16550Tty<B> {
    /// Returns a reference to the underlying [`Uart16550`].
    pub const fn inner(&self) -> &Uart16550<B> {
        &self.0
    }

    /// Returns a mutable reference to the underlying [`Uart16550`].
    pub const fn inner_mut(&mut self) -> &mut Uart16550<B> {
        &mut self.0
    }
}

impl<B: Backend> fmt::Write for Uart16550Tty<B> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &byte in s.as_bytes() {
            match byte {
                // backspace or delete
                8 | 0x7F => {
                    self.0.send_bytes_exact(&[8]);
                    self.0.send_bytes_exact(b" ");
                    self.0.send_bytes_exact(&[8]);
                }
                // Normal Rust newlines to terminal-compatible newlines.
                b'\n' => {
                    self.0.send_bytes_exact(b"\r");
                    self.0.send_bytes_exact(b"\n");
                }
                data => {
                    self.0.send_bytes_exact(&[data]);
                }
            }
        }

        Ok(())
    }
}
