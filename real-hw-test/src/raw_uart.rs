use core::arch::asm;

use crate::device::Address;

#[derive(Clone, Copy, Debug)]
pub struct RawUart {
    address: Address,
}

impl RawUart {
    pub const fn new(address: Address) -> Self {
        Self { address }
    }

    pub unsafe fn read(&mut self, offset: u8) -> u8 {
        debug_assert!(offset < 8);
        match self.address {
            Address::Port(base) => {
                let port = base + u16::from(offset);
                let value: u8;
                // SAFETY: ownership of the discovered UART was established before probing.
                unsafe {
                    asm!(
                        "in al, dx",
                        in("dx") port,
                        out("al") value,
                        options(nomem, nostack, preserves_flags)
                    );
                }
                value
            }
            Address::Mmio { base, stride } => {
                let address = base + usize::from(offset) * usize::from(stride);
                // SAFETY: the firmware-provided MMIO range is live while boot services are active.
                unsafe { core::ptr::read_volatile(address as *const u8) }
            }
        }
    }

    pub unsafe fn write(&mut self, offset: u8, value: u8) {
        debug_assert!(offset < 8);
        match self.address {
            Address::Port(base) => {
                let port = base + u16::from(offset);
                // SAFETY: ownership of the discovered UART was established before probing.
                unsafe {
                    asm!(
                        "out dx, al",
                        in("dx") port,
                        in("al") value,
                        options(nomem, nostack, preserves_flags)
                    );
                }
            }
            Address::Mmio { base, stride } => {
                let address = base + usize::from(offset) * usize::from(stride);
                // SAFETY: the firmware-provided MMIO range is live while boot services are active.
                unsafe { core::ptr::write_volatile(address as *mut u8, value) };
            }
        }
    }

    pub fn scratch_test(&mut self) -> bool {
        const SPR: u8 = 7;

        // The old value is restored because firmware may use the scratch register diagnostically.
        let old = unsafe { self.read(SPR) };
        let passed = [0x42, 0x73].into_iter().all(|pattern| unsafe {
            self.write(SPR, pattern);
            self.read(SPR) == pattern
        });
        unsafe { self.write(SPR, old) };
        passed
    }
}
