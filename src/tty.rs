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
#[cfg(any(target_arch = "x86", target_arch = "x86_64", doc))]
use crate::backend::{PioBackend, PortIoAddress};
use crate::{Config, InitError, InvalidAddressError, LoopbackError, Uart16550};
use core::error::Error;
use core::fmt::{self, Display, Formatter};
use core::ptr::NonNull;

/// Errors that [`Uart16550Tty::new_port()`] and [`Uart16550Tty::new_mmio()`] may
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

/// Lightweight, opinionated wrapper around [`Uart16550`] for sending Rust
/// strings to a connected TTY (terminal).
///
/// Ideal for quickly observing debug output during VM development and testing.
/// It implements [`fmt::Write`] allowing the use of `write!()`.
///
/// Access to the underlying UART device is provided through via
/// [`Uart16550Tty::inner()`] and [`Uart16550Tty::inner_mut()`].
///
/// # Example (x86 Port IO)
///
#[cfg_attr(
    any(target_arch = "x86", target_arch = "x86_64"),
    doc = "```rust,no_run"
)]
#[cfg_attr(
    not(any(target_arch = "x86", target_arch = "x86_64")),
    doc = "```rust,ignore"
)]
/// use uart_16550::{Config, Uart16550Tty};
/// use core::fmt::Write;
///
/// // SAFETY: The port is valid and we have exclusive access.
/// let mut uart = unsafe { Uart16550Tty::new_port(0x3f8, Config::default()).expect("should initialize device") };
/// uart.write_str("hello world\nhow's it going?");
/// ```
///
/// # Example (MMIO)
///
/// ```rust,no_run
/// use uart_16550::{Config, Uart16550Tty};
/// use core::fmt::Write;
/// use core::ptr::{self, NonNull};
///
/// let mmio_address = ptr::with_exposed_provenance_mut::<u8>(0x1000);
/// let mmio_address = NonNull::new(mmio_address).unwrap();
///
/// // SAFETY: The address is valid and we have exclusive access.
/// let mut uart = unsafe { Uart16550Tty::new_mmio(mmio_address, 4, Config::default()).expect("should initialize device") };
/// uart.write_str("hello world\nhow's it going?");
/// ```
///
/// # MMIO and Port I/O
///
/// Uart 16550 devices are typically mapped via port I/O on x86 and via MMIO on
/// other platforms. The constructors [`Uart16550Tty::new_port()`] and
/// [`Uart16550Tty::new_mmio()`] create an instance of a device with the
/// corresponding backend.
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

#[cfg(any(target_arch = "x86", target_arch = "x86_64", doc))]
impl Uart16550Tty<PioBackend> {
    /// Creates a new [`Uart16550Tty`] backed by x86 port I/O.
    ///
    /// Initializes the device and performs a self-test.
    ///
    /// # Arguments
    ///
    /// - `base_address`: Base address of the UART.
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
    /// # Arguments
    ///
    /// - `base_address`: Base address of the UART.
    /// - `stride`: The stride is the fixed byte distance in physical address
    ///   space between consecutive logical registers, i.e. how much the address
    ///   increases when moving from one register index to the next. Typical
    ///   values are `1`, `2`, `4`, and `8` - depending on your hardware/board.
    ///
    /// # Safety
    ///
    /// Callers must ensure that the base address is valid and safe to use for
    /// the **whole lifetime** of the device. Further, all [`NUM_REGISTERS`]
    /// registers must be safely reachable from the base address.
    ///
    /// [`NUM_REGISTERS`]: crate::spec::NUM_REGISTERS
    pub unsafe fn new_mmio(
        base_address: NonNull<u8>,
        stride: u8,
        config: Config,
    ) -> Result<Self, Uart16550TtyError<MmioAddress>> {
        // SAFETY: The address is valid and we have exclusive access.
        let mut inner = unsafe {
            Uart16550::new_mmio(base_address, stride).map_err(Uart16550TtyError::AddressError)?
        };

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
