//! x86 Port IO backend implementation.

use super::{Backend, RegisterAddress, assert_offset, private};
use core::arch::asm;

/// x86 port I/O address.
///
/// See [`RegisterAddress`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct PortIoAddress(pub(crate) u16);

impl RegisterAddress for PortIoAddress {
    #[inline(always)]
    fn add_offset(self, offset: u8) -> Self {
        let port = self.0 + offset as u16;
        Self(port)
    }
}

impl private::Sealed for PortIoAddress {}

/// x86 Port I/O backed UART 16550.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct PioBackend(pub(crate) PortIoAddress /* base port */);

impl private::Sealed for PioBackend {}

impl PioBackend {
    /// Returns the base port.
    #[must_use]
    pub const fn base(&self) -> PortIoAddress {
        self.0
    }

    /// Returns the port of the register at the given offset.
    #[inline(always)]
    fn port(&self, offset: u8) -> PortIoAddress {
        assert_offset(offset);
        self.0.add_offset(offset)
    }
}

impl Backend for PioBackend {
    #[inline(always)]
    unsafe fn read(&mut self, offset: u8) -> u8 {
        let port = self.port(offset);
        // SAFETY: The constructor ensured that the I/O port is safe to use.
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
    unsafe fn write(&mut self, offset: u8, value: u8) {
        let port = self.port(offset);
        // SAFETY: The constructor ensured that the I/O port is safe to use.
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
