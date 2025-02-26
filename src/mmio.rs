use core::ptr::NonNull;

use volatile::VolatileRef;

use crate::{register::Uart16550Register, uart_16550::Uart16550Registers};

/// Basically a pointer to a memory mapped register of a 16550
pub struct MemoryMappedRegister<'a> {
    volatile_ref: VolatileRef<'a, u8>,
}

impl MemoryMappedRegister<'_> {
    unsafe fn new(ptr: NonNull<u8>) -> Self {
        Self {
            volatile_ref: VolatileRef::new(ptr),
        }
    }
}

impl Uart16550Register for MemoryMappedRegister<'_> {
    fn read(&self) -> u8 {
        self.volatile_ref.as_ptr().read()
    }

    fn write(&mut self, value: u8) {
        self.volatile_ref.as_mut_ptr().write(value);
    }
}

/// ## Safety
///
/// - The pointer must map to the base register of a correctly memory mapped 16550.
/// - The stride must match the actual stride that is memory mapped.
/// - The pointer must be properly aligned.
/// - It must be “dereferenceable” in the sense defined in the [`core::ptr`] documentation.
/// - The pointer must point to an initialized instance of T.
/// - You must enforce Rust’s aliasing rules, since the returned lifetime 'a is arbitrarily
///   chosen and does not necessarily reflect the actual lifetime of the data. In particular,
///   while this `VolatileRef` exists, the memory the pointer points to must not get accessed
///   (_read or written_) through any other pointer.
pub unsafe fn new<'a>(
    base_pointer: NonNull<u8>,
    stride: usize,
) -> Uart16550Registers<MemoryMappedRegister<'a>> {
    #[allow(clippy::identity_op)]
    Uart16550Registers {
        data: MemoryMappedRegister::new(base_pointer),
        int_en: MemoryMappedRegister::new(base_pointer.add(1 * stride)),
        fifo_ctrl: MemoryMappedRegister::new(base_pointer.add(2 * stride)),
        line_ctrl: MemoryMappedRegister::new(base_pointer.add(3 * stride)),
        modem_ctrl: MemoryMappedRegister::new(base_pointer.add(4 * stride)),
        line_sts: MemoryMappedRegister::new(base_pointer.add(5 * stride)),
    }
}
