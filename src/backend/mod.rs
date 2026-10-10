// SPDX-License-Identifier: MIT OR Apache-2.0

//! Abstraction over the I/O backend (Hardware Abstraction Layer (HAL)).
//!
//! Main exports:
//! - [`Backend`]
//! - [`PioBackend`]
//! - [`MmioBackend`]
//!
//! # Custom Backends
//!
//! Besides the built-in backends, you can implement [`Backend`] yourself and
//! pass it to [`Uart16550::new_with_backend()`]. This is useful when registers
//! can't be accessed with plain `in`/`out` instructions or volatile memory
//! accesses, e.g., in confidential VMs (Intel TDX, AMD SEV-SNP), where port
//! I/O goes through a `tdcall` or the GHCB protocol.
//!
//! ```rust,no_run
//! use uart_16550::backend::Backend;
//! use uart_16550::{Config, Uart16550};
//!
//! /// Port I/O through a hypervisor interface.
//! struct HypercallPio {
//!     base: u16,
//! }
//!
//! # fn hypercall_io_in(_port: u16) -> u8 { 0 }
//! # fn hypercall_io_out(_port: u16, _value: u8) {}
//! impl Backend for HypercallPio {
//!     unsafe fn read(&mut self, register_offset: u8) -> u8 {
//!         hypercall_io_in(self.base + u16::from(register_offset))
//!     }
//!
//!     unsafe fn write(&mut self, register_offset: u8, value: u8) {
//!         hypercall_io_out(self.base + u16::from(register_offset), value)
//!     }
//! }
//!
//! // SAFETY: The port is valid and we have exclusive access.
//! let mut uart = unsafe { Uart16550::new_with_backend(HypercallPio { base: 0x3f8 }) };
//! uart.init(Config::default()).expect("should init device successfully");
//! uart.send_bytes_exact(b"hello world!");
//! ```
//!
//! [`Uart16550::new_with_backend()`]: crate::Uart16550::new_with_backend

mod mmio;
#[cfg(any(target_arch = "x86", target_arch = "x86_64", doc))]
mod pio;

pub use mmio::MmioBackend;
#[cfg(any(target_arch = "x86", target_arch = "x86_64", doc))]
pub use pio::PioBackend;

use crate::spec::NUM_REGISTERS;

#[inline]
#[track_caller]
fn assert_offset(register_offset: u8) {
    assert!(
        register_offset < NUM_REGISTERS as u8,
        "the register offset should be within the expected range: expected {register_offset} to be less than {NUM_REGISTERS}",
    );
}

/// Abstraction over the I/O backend of a UART 16550 microcontroller.
///
/// This acts as Hardware Abstraction Layer (HAL) and abstracts over x86 port
/// I/O and generic MMIO.
///
/// # Register Offsets
///
/// Registers are identified by their logical register offset, i.e., their
/// index from `0` to `NUM_REGISTERS - 1`. It doesn't depend on how the
/// registers are laid out in the address space: applying a stride, for
/// example, is up to the backend.
///
/// Responsibilities for valid register offsets:
///
/// - Callers of [`Backend::read`] and [`Backend::write`] must pass a register
///   offset less than [`NUM_REGISTERS`]. This is the safety contract of both
///   methods.
/// - [`Uart16550`] only ever passes valid register offsets. Callers of raw
///   accesses via [`Uart16550::backend_mut()`] are responsible themselves.
/// - Implementations can therefore rely on valid register offsets. The
///   built-in backends still check them and panic otherwise.
///
/// That all registers fit into the address space is checked once, when the
/// device is created: the constructors of [`Uart16550`] return an error
/// otherwise.
///
/// [`Uart16550`]: crate::Uart16550
/// [`Uart16550::backend_mut()`]: crate::Uart16550::backend_mut
pub trait Backend {
    /// Reads one byte from the register at the given register offset.
    ///
    /// This needs a mutable reference as reads can have side effects on the
    /// device, depending on the register.
    ///
    /// # Arguments
    ///
    /// - `register_offset`: The logical register offset, without any stride
    ///   applied. See [`Backend`].
    ///
    /// # Safety
    ///
    /// Callers must ensure that `register_offset` is less than
    /// [`NUM_REGISTERS`]. See [`Backend`] for who is responsible when.
    unsafe fn read(&mut self, register_offset: u8) -> u8;

    /// Writes one byte to the register at the given register offset.
    ///
    /// Writes can have side effects on the device, depending on the register.
    ///
    /// # Arguments
    ///
    /// - `register_offset`: The logical register offset, without any stride
    ///   applied. See [`Backend`].
    ///
    /// # Safety
    ///
    /// Callers must ensure that `register_offset` is less than
    /// [`NUM_REGISTERS`]. See [`Backend`] for who is responsible when.
    unsafe fn write(&mut self, register_offset: u8, value: u8);
}
