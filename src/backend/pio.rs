//! x86 Port IO backend implementation.

use super::{Backend, assert_offset, private};
use core::arch::asm;

/// x86 Port I/O backed UART 16550.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct PioBackend {
    pub(crate) base: u16,
}

impl private::Sealed for PioBackend {}

impl PioBackend {
    /// Returns the base port.
    #[must_use]
    pub const fn base(&self) -> u16 {
        self.base
    }

    /// Returns the port of the register at the given register offset.
    #[inline(always)]
    fn port(&self, register_offset: u8) -> u16 {
        assert_offset(register_offset);
        self.base
            .checked_add(u16::from(register_offset))
            .expect("register port should not overflow, as checked by the constructor")
    }
}

impl Backend for PioBackend {
    #[inline(always)]
    unsafe fn read(&mut self, register_offset: u8) -> u8 {
        let port = self.port(register_offset);
        // SAFETY: The constructor ensured that the I/O port is safe to use.
        unsafe {
            let ret: u8;
            asm!(
                "inb %dx, %al",
                in("dx") port,
                out("al") ret,
                options(att_syntax, nostack, preserves_flags)
            );
            ret
        }
    }

    #[inline(always)]
    unsafe fn write(&mut self, register_offset: u8, value: u8) {
        let port = self.port(register_offset);
        // SAFETY: The constructor ensured that the I/O port is safe to use.
        unsafe {
            asm!(
                "outb %al, %dx",
                in("al") value,
                in("dx") port,
                options(att_syntax, nostack, preserves_flags)
            );
        }
    }
}
