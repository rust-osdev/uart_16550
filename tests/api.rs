use core::num::NonZeroUsize;
use core::ptr::{self, NonNull};
use std::sync::Mutex;
use uart_16550::Uart16550;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use uart_16550::backend::PioBackend;
use uart_16550::backend::{Backend, MmioBackend};

/// This ensures that all necessary helper types to create bindings are publicly
/// exported.
///
/// This test succeeds if it compiles.
#[test]
fn test_public_api() {
    fn consume(_device: Uart16550<impl Backend>) {}

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // SAFETY: This is a synthetic example and the hardware is not accessed.
        let device: Uart16550<PioBackend> = unsafe { Uart16550::new_port(0x3f8) }.unwrap();
        consume(device);
    }
    let mmio_address = ptr::with_exposed_provenance_mut::<u8>(0x1000);
    let mmio_address = NonNull::new(mmio_address).unwrap();
    // SAFETY: This is a synthetic example and the hardware is not accessed.
    let device: Uart16550<MmioBackend> = unsafe { Uart16550::new_mmio(mmio_address, 1) }.unwrap();
    consume(device);
}

/// A port I/O device can be placed in a `static`.
///
/// This test succeeds if it compiles.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn test_static_port() {
    static UART: Mutex<Uart16550<PioBackend>> = Mutex::new(
        // SAFETY: This is a synthetic example and the hardware is not accessed.
        match unsafe { Uart16550::new_port(0x3f8) } {
            Ok(uart) => uart,
            Err(_) => panic!("should be a valid port"),
        },
    );
    let _ = &UART;
}

/// An MMIO device can be placed in a `static`.
///
/// This test succeeds if it compiles.
#[test]
fn test_static_mmio() {
    static UART: Mutex<Uart16550<MmioBackend>> = Mutex::new(
        // SAFETY: This is a synthetic example and the hardware is not accessed.
        match unsafe { Uart16550::new_mmio_from_address(NonZeroUsize::new(0x1000).unwrap(), 4) } {
            Ok(uart) => uart,
            Err(_) => panic!("should be a valid address"),
        },
    );
    let _ = &UART;
}
