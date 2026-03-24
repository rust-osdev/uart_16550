//! x86 Port IO backend implementation.

use super::{Backend, RegisterAddress, private};
use core::arch::asm;
use core::num::NonZeroU8;

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

impl Backend for PioBackend {
    type Address = PortIoAddress;

    #[inline(always)]
    fn base(&self) -> Self::Address {
        self.0
    }

    #[inline(always)]
    fn stride(&self) -> NonZeroU8 {
        // stride=1: x86 port I/O registers are always at consecutive port
        // numbers. Compiler optimizes the unwrap away.
        NonZeroU8::new(1).unwrap()
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
