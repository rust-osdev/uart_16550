//! MMIO backend implementation.

use super::{Backend, assert_offset, private};
use core::num::NonZeroU8;
use core::ptr::NonNull;

/// Arch-specific quirks to access hardware.
///
/// On MMIO-access on aarch64, LLVM may emit instructions that are not properly
/// virtualizable. We therefore need to be more explicit about the instruction.
/// More info: <https://github.com/rust-lang/rust/issues/131894>
mod arch {
    #[cfg(any(doc, not(target_arch = "aarch64")))]
    use core::ptr;

    /// Wrapper around [`ptr::read_volatile`].
    #[cfg(target_arch = "aarch64")]
    #[inline(always)]
    pub unsafe fn mmio_read_register(ptr: *mut u8) -> u8 {
        let ret: u8;
        // SAFETY: Caller ensures the address is valid MMIO memory.
        unsafe {
            core::arch::asm!(
                "ldrb {ret:w}, [{ptr}]",
                ptr = in(reg) ptr,
                ret = out(reg) ret,
                options(nostack, preserves_flags)
            );
        }
        ret
    }

    /// Wrapper around [`ptr::read_volatile`].
    #[cfg(not(target_arch = "aarch64"))]
    #[inline(always)]
    pub unsafe fn mmio_read_register(ptr: *mut u8) -> u8 {
        // SAFETY: Caller ensures the address is valid MMIO memory.
        unsafe { ptr::read_volatile(ptr) }
    }

    /// Wrapper around [`ptr::write_volatile`].
    #[cfg(target_arch = "aarch64")]
    #[inline(always)]
    pub unsafe fn mmio_write_register(ptr: *mut u8, value: u8) {
        // SAFETY: Caller ensures the address is valid MMIO memory.
        unsafe {
            core::arch::asm!(
                "strb {val:w}, [{ptr}]",
                val = in(reg) value,
                ptr = in(reg) ptr,
                options(nostack, preserves_flags)
            );
        }
    }

    /// Wrapper around [`ptr::write_volatile`].
    #[cfg(not(target_arch = "aarch64"))]
    #[inline(always)]
    pub unsafe fn mmio_write_register(ptr: *mut u8, value: u8) {
        // SAFETY: Caller ensures the address is valid MMIO memory.
        unsafe { ptr::write_volatile(ptr, value) }
    }
}

/// MMIO-mapped UART 16550.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct MmioBackend {
    pub(crate) base: NonNull<u8>,
    pub(crate) stride: NonZeroU8,
}

// SAFETY: `Uart16550` is not `Sync`, so concurrent access from multiple
// threads is not possible through this type's API alone. Implementing `Send`
// allows moving ownership to another thread, which is safe because at any
// point only one thread holds the `&mut self` required for all operations.
// Without this, higher-level wrappers such as `Mutex<Uart16550>` could not
// be constructed, since `Mutex<T>: Sync` requires `T: Send`.
unsafe impl Send for MmioBackend {}

impl private::Sealed for MmioBackend {}

impl MmioBackend {
    /// Returns the base address.
    #[must_use]
    pub const fn base(&self) -> NonNull<u8> {
        self.base
    }

    /// Returns the stride, i.e., the distance between consecutive registers.
    #[must_use]
    pub const fn stride(&self) -> NonZeroU8 {
        self.stride
    }

    /// Returns the address of the register at the given register offset.
    #[inline(always)]
    fn register(&self, register_offset: u8) -> *mut u8 {
        assert_offset(register_offset);
        let offset = usize::from(register_offset) * usize::from(self.stride.get());
        let address = self
            .base
            .addr()
            .get()
            .checked_add(offset)
            .expect("register address should not overflow, as checked by the constructor");
        self.base.as_ptr().with_addr(address)
    }
}

impl Backend for MmioBackend {
    #[inline(always)]
    unsafe fn read(&mut self, register_offset: u8) -> u8 {
        let ptr = self.register(register_offset);
        // SAFETY: The constructor ensured that the MMIO address is safe to use.
        unsafe { arch::mmio_read_register(ptr) }
    }

    #[inline(always)]
    unsafe fn write(&mut self, register_offset: u8, value: u8) {
        let ptr = self.register(register_offset);
        // SAFETY: The constructor ensured that the MMIO address is safe to use.
        unsafe { arch::mmio_write_register(ptr, value) }
    }
}

#[cfg(test)]
mod tests {
    use crate::Uart16550;
    use crate::spec::NUM_REGISTERS;
    use core::num::NonZeroUsize;
    use core::ptr::{self, NonNull};

    /// All registers at the highest base address the constructors accept are
    /// addressable without overflow.
    #[test]
    fn register_at_upper_boundary() {
        for stride in [1_u8, 2, 4, 8] {
            let base = usize::MAX - (NUM_REGISTERS - 1) * usize::from(stride);
            let ptr = NonNull::new(ptr::with_exposed_provenance_mut::<u8>(base))
                .expect("should be non-null");

            // SAFETY: We just test the constructor but do not access the device.
            let uart = unsafe { Uart16550::new_mmio(ptr, stride) }
                .expect("should accept the highest valid base address");
            let backend = uart.backend();
            for register_offset in 0..NUM_REGISTERS as u8 {
                assert_eq!(
                    backend.register(register_offset).addr(),
                    base + usize::from(register_offset) * usize::from(stride)
                );
            }
            assert_eq!(backend.register(NUM_REGISTERS as u8 - 1).addr(), usize::MAX);

            // SAFETY: We just test the constructors but do not access the
            // device.
            unsafe {
                let address = NonZeroUsize::new(base).expect("should be non-zero");
                assert!(Uart16550::new_mmio_from_address(address, stride).is_ok());

                let address = address.checked_add(1).expect("should not overflow");
                let ptr = NonNull::new(ptr::with_exposed_provenance_mut::<u8>(address.get()))
                    .expect("should be non-null");
                assert!(Uart16550::new_mmio(ptr, stride).is_err());
                assert!(Uart16550::new_mmio_from_address(address, stride).is_err());
            }
        }
    }
}
