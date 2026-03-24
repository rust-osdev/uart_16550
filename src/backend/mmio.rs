//! MMIO backend implementation.

use super::{Backend, RegisterAddress, private};
use crate::spec::NUM_REGISTERS;
use core::num::NonZeroU8;
use core::ptr::NonNull;

/// Memory-mapped I/O (MMIO) address.
///
/// Guaranteed to be not null.
///
/// See [`RegisterAddress`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct MmioAddress(pub(crate) NonNull<u8>);

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

impl private::Sealed for MmioAddress {}

/// Arch-specific quirks to access hardware.
///
/// On MMIO-access on aarch64, LLVM may emit instructions that are not properly
/// virtualizable. We therefore need to be more explicit about the instruction.
/// More info: <https://github.com/rust-lang/rust/issues/131894>
mod arch {
    use super::MmioAddress;
    #[cfg(any(doc, not(target_arch = "aarch64")))]
    use core::ptr;

    /// Wrapper around [`ptr::read_volatile`].
    #[cfg(target_arch = "aarch64")]
    #[inline(always)]
    pub unsafe fn mmio_read_register(address: MmioAddress) -> u8 {
        let ptr = address.0.as_ptr();
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
    pub unsafe fn mmio_read_register(address: MmioAddress) -> u8 {
        // SAFETY: Caller ensures the address is valid MMIO memory.
        unsafe { ptr::read_volatile(address.0.as_ptr()) }
    }

    /// Wrapper around [`ptr::write_volatile`].
    #[cfg(target_arch = "aarch64")]
    #[inline(always)]
    pub unsafe fn mmio_write_register(address: MmioAddress, value: u8) {
        let ptr = address.0.as_ptr();
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
    pub unsafe fn mmio_write_register(address: MmioAddress, value: u8) {
        // SAFETY: Caller ensures the address is valid MMIO memory.
        unsafe { ptr::write_volatile(address.0.as_ptr(), value) }
    }
}

/// MMIO-mapped UART 16550.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct MmioBackend {
    // non-null
    pub(crate) base_address: MmioAddress,
    pub(crate) stride: NonZeroU8,
}

impl private::Sealed for MmioBackend {}

impl Backend for MmioBackend {
    type Address = MmioAddress;

    #[inline(always)]
    fn base(&self) -> Self::Address {
        self.base_address
    }

    #[inline(always)]
    fn stride(&self) -> NonZeroU8 {
        self.stride
    }

    #[inline(always)]
    unsafe fn _read_register(&mut self, address: MmioAddress) -> u8 {
        debug_assert!(address >= self.base());
        let upper_bound_incl = (NUM_REGISTERS - 1) * usize::from(u8::from(self.stride));
        // Address is in the device's address range
        debug_assert!(address.0.as_ptr() <= self.base().0.as_ptr().wrapping_add(upper_bound_incl));

        // SAFETY: The caller ensured that the MMIO address is safe to use.
        unsafe { arch::mmio_read_register(address) }
    }

    #[inline(always)]
    unsafe fn _write_register(&mut self, address: MmioAddress, value: u8) {
        debug_assert!(address >= self.base());
        let upper_bound_incl = (NUM_REGISTERS - 1) * usize::from(u8::from(self.stride));
        // Address is in the device's address range
        debug_assert!(address.0.as_ptr() <= self.base().0.as_ptr().wrapping_add(upper_bound_incl));

        // SAFETY: The caller ensured that the MMIO address is safe to use.
        unsafe { arch::mmio_write_register(address, value) }
    }
}
