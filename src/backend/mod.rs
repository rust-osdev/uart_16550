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

pub use mmio::{MmioAddress, MmioBackend};
#[cfg(any(target_arch = "x86", target_arch = "x86_64", doc))]
pub use pio::{PioBackend, PortIoAddress};

use crate::spec::NUM_REGISTERS;
use core::fmt::Debug;
use core::num::NonZeroU8;

mod private {
    pub trait Sealed {}
}

/// Abstraction over register addresses in [`Backend`].
///
/// # Safety
///
/// All implementations and instances of this trait are created within this
/// crate and do follow all safety invariants. API users don't get access to the
/// underlying register addresses, nor can they construct one themselves, as this
/// type et al. are sealed.
pub trait RegisterAddress: Copy + Clone + Debug + Sized + private::Sealed {
    /// Adds a byte offset onto the base register address.
    fn add_offset(self, offset: u8) -> Self;
}

#[track_caller]
fn assert_offset(offset: u8) {
    assert!(
        offset < NUM_REGISTERS as u8,
        "the offset should be within the expected range: expected {offset} to be less than {NUM_REGISTERS}",
    );
}

/// Abstraction over the I/O backend of a UART 16550 microcontroller.
///
/// This acts as Hardware Abstraction Layer (HAL) and abstracts over x86 port
/// I/O and generic MMIO.
///
/// Users should use [`Backend::read`] and [`Backend::write`].
pub trait Backend: Send + private::Sealed {
    /// The [`RegisterAddress`] that naturally belongs to the [`Backend`].
    type Address: RegisterAddress;

    /* convenience with default impl */

    /// Reads one byte from the specified register at the given offset.
    ///
    /// This needs a mutable reference as reads can have side effects on the
    /// device, depending on the register.
    ///
    /// # Arguments
    ///
    /// - `offset`: The register offset regarding the base register. The offset
    ///   **must** be less than [`NUM_REGISTERS`].
    ///
    /// # Safety
    ///
    /// Callers must ensure that the effective address consisting of
    /// [`Self::base`] and `offset` is valid and safe to read.
    #[inline(always)]
    unsafe fn read(&mut self, offset: u8) -> u8 {
        assert_offset(offset);
        let address_offset = offset
            .checked_mul(u8::from(self.stride()))
            .expect("offset * stride overflows u8; reduce stride");
        let addr = self.base().add_offset(address_offset);
        // SAFETY: The caller ensured that the register address is safe to use.
        unsafe { self._read_register(addr) }
    }

    /// Writes one byte to the specified register at the given offset.
    ///
    /// Writes can have side effects on the device, depending on the register.
    ///
    /// # Arguments
    ///
    /// - `offset`: The register offset regarding the base register. The offset
    ///   **must** be less than [`NUM_REGISTERS`].
    ///
    /// # Safety
    ///
    /// Callers must ensure that the effective address consisting of
    /// [`Self::base`] and `offset` is valid and safe to write.
    #[inline(always)]
    unsafe fn write(&mut self, offset: u8, value: u8) {
        assert_offset(offset);
        let address_offset = offset
            .checked_mul(u8::from(self.stride()))
            .expect("offset * stride overflows u8; reduce stride");
        let addr = self.base().add_offset(address_offset);
        // SAFETY: The caller ensured that the register address is safe to use.
        unsafe { self._write_register(addr, value) }
    }

    /* needs impl */

    /// Returns the base [`RegisterAddress`].
    fn base(&self) -> Self::Address;

    /// Returns the configured stride.
    ///
    /// The stride is the fixed byte distance in physical address space between
    /// consecutive logical registers, i.e. how much the address increases when
    /// moving from one register index to the next.
    fn stride(&self) -> NonZeroU8;

    /// PRIVATE API! Use [`Self::read`]!
    ///
    /// Reads one byte from the specified register.
    ///
    /// This needs a mutable reference as reads can have side effects on the
    /// device, depending on the register.
    ///
    /// # Arguments
    ///
    /// - `address`: The total address of the register.
    ///
    /// # Safety
    ///
    /// Callers must ensure that the provided address is valid and safe to read.
    #[doc(hidden)]
    unsafe fn _read_register(&mut self, address: Self::Address) -> u8;

    /// PRIVATE API! Use [`Self::write`]!
    ///
    /// Writes one byte to the specified register.
    ///
    /// Writes can have side effects on the device, depending on the register.
    ///
    /// # Arguments
    ///
    /// - `address`: The total address of the register.
    ///
    /// # Safety
    ///
    /// Callers must ensure that the provided address is valid and safe to write.
    #[doc(hidden)]
    unsafe fn _write_register(&mut self, address: Self::Address, value: u8);
}
