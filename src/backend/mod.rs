// SPDX-License-Identifier: MIT OR Apache-2.0

//! Abstraction over the I/O backend (Hardware Abstraction Layer (HAL)).
//!
//! Main exports:
//! - [`Backend`]
//! - [`PioBackend`]
//! - [`MmioBackend`]

mod mmio;
#[cfg(any(target_arch = "x86", target_arch = "x86_64", doc))]
mod pio;

pub use mmio::MmioBackend;
#[cfg(any(target_arch = "x86", target_arch = "x86_64", doc))]
pub use pio::PioBackend;

use crate::spec::NUM_REGISTERS;

mod private {
    pub trait Sealed {}
}

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
/// Registers are identified by their logical register offset, i.e., their
/// index from `0` to `NUM_REGISTERS - 1`. It doesn't depend on how the
/// registers are laid out in the address space: applying a stride, for
/// example, is up to the backend.
pub trait Backend: Send + private::Sealed {
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
    /// [`NUM_REGISTERS`].
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
    /// [`NUM_REGISTERS`].
    unsafe fn write(&mut self, register_offset: u8, value: u8);
}
