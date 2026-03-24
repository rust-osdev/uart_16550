use uart_16550::backend::{Backend, MmioBackend};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use uart_16550::backend::PioBackend;
use uart_16550::Uart16550;


/// This ensures that all necessary helper types to create bindings are publicly
/// exported.
///
/// This test succeeds if it compiles.
#[test]
fn test_public_api() {
    fn consume(_device: Uart16550<impl Backend>) {    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // SAFETY: This is a synthetic example and the hardware is not accessed.
        let device: Uart16550<PioBackend> = unsafe { Uart16550::new_port(0x3f8) }.unwrap();
        consume(device);
    }
    // SAFETY: This is a synthetic example and the hardware is not accessed.
    let device: Uart16550<MmioBackend> = unsafe { Uart16550::new_mmio(0x1000 as *mut _, 1) }.unwrap();
    consume(device);
}
