// SPDX-License-Identifier: MIT OR Apache-2.0

//! Abstraction over the I/O backend (Hardware Abstraction Layer (HAL)).
//!
//! Main exports:
//! - [`Backend`]
//! - [`PioBackend`]
//! - [`MmioBackend`]

use crate::spec::NUM_REGISTERS;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use core::arch::asm;
use core::fmt::Debug;
use core::ptr::{self, read_volatile, write_volatile};

/// Abstraction over register addresses in [`Backend`].
pub trait RegisterAddress: Copy + Clone + Debug + Sized {
    /// Adds a byte offset onto the base register address.
    fn add_offset(self, offset: u8) -> Self;
}

/// x86 port I/O address.
///
/// See [`RegisterAddress`].
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct PortIoAddress(pub(crate) u16);

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl RegisterAddress for PortIoAddress {
    #[inline(always)]
    fn add_offset(self, offset: u8) -> Self {
        let port = self.0 + offset as u16;
        Self(port)
    }
}
/// Memory-mapped I/O (MMIO) address.
///
/// Guaranteed to be not null.
///
/// See [`RegisterAddress`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct MmioAddress(pub(crate) *mut u8);

// SAFETY: `Uart16550` is not `Sync`, so concurrent access from multiple
// threads is not possible through this type's API alone. Implementing `Send`
// allows moving ownership to another thread, which is safe because at any
// point only one thread holds the `&mut self` required for all operations.
// Without this, higher-level wrappers such as `Mutex<Uart16550>` could not
// be constructed, since `Mutex<T>: Sync` requires `T: Send`.
unsafe impl Send for MmioAddress {}

impl RegisterAddress for MmioAddress {
    #[inline(always)]
    fn add_offset(self, offset: u8) -> Self {
        // SAFETY: We ensure on a higher level that the base address is valid
        // and that this will not wrap.
        let address = unsafe { self.0.add(offset as usize) };
        Self(address)
    }
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
pub trait Backend: Send {
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
        let addr = self.base().add_offset(offset);
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
        let addr = self.base().add_offset(offset);
        // SAFETY: The caller ensured that the register address is safe to use.
        unsafe { self._write_register(addr, value) }
    }

    /* needs impl */

    /// Returns the base [`RegisterAddress`].
    fn base(&self) -> Self::Address;

    /// PRIVATE API!
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

    /// PRIVATE API!
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

/// x86 Port I/O backed UART 16550.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Hash)]
pub struct PioBackend(pub(crate) PortIoAddress /* base port */);

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Backend for PioBackend {
    type Address = PortIoAddress;

    fn base(&self) -> Self::Address {
        self.0
    }

    #[inline(always)]
    unsafe fn _read_register(&mut self, port: PortIoAddress) -> u8 {
        // SAFETY: The caller ensured that the I/O port is safe to use.
        unsafe {
            let ret: u8;
            asm!(
                "inb %dx, %al",
                in("dx") port.0,
                out("al") ret,
                options(att_syntax, nostack, preserves_flags)
            );
            ret
        }
    }

    #[inline(always)]
    unsafe fn _write_register(&mut self, port: PortIoAddress, value: u8) {
        // SAFETY: The caller ensured that the I/O port is safe to use.
        unsafe {
            asm!(
                "outb %al, %dx",
                in("al") value,
                in("dx") port.0,
                options(att_syntax, nostack, preserves_flags)
            );
        }
    }
}

/// MMIO-mapped UART 16550.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Hash)]
pub struct MmioBackend(pub(crate) MmioAddress /* base address, non-null */);

impl Backend for MmioBackend {
    type Address = MmioAddress;

    fn base(&self) -> Self::Address {
        self.0
    }

    #[inline(always)]
    unsafe fn _read_register(&mut self, address: MmioAddress) -> u8 {
        // SAFETY: The caller ensured that the MMIO address is safe to use.
        unsafe { read_volatile(address.0) }
    }

    #[inline(always)]
    unsafe fn _write_register(&mut self, address: MmioAddress, value: u8) {
        // SAFETY: The caller ensured that the MMIO address is safe to use.
        unsafe { write_volatile(address.0, value) }
    }
}
