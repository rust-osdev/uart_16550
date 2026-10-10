use core::num::NonZeroUsize;
use core::ptr::{self, NonNull};
use std::sync::Mutex;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use uart_16550::backend::PioBackend;
use uart_16550::backend::{Backend, MmioBackend};
use uart_16550::spec::NUM_REGISTERS;
use uart_16550::spec::registers::{LCR, LSR, offsets};
use uart_16550::{Config, Uart16550, Uart16550Tty};

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

/// A [`Uart16550Tty`] can wrap a device without accessing it, also in a
/// `static`.
///
/// This test succeeds if it compiles.
#[test]
fn test_static_tty() {
    static TTY: Mutex<Uart16550Tty<MmioBackend>> = Mutex::new(Uart16550Tty::from_inner(
        // SAFETY: This is a synthetic example and the hardware is not accessed.
        match unsafe { Uart16550::new_mmio_from_address(NonZeroUsize::new(0x1000).unwrap(), 4) } {
            Ok(uart) => uart,
            Err(_) => panic!("should be a valid address"),
        },
    ));
    let _ = &TTY;
}

/// A custom backend operating on an in-memory register file.
///
/// Reads and writes are passed through, except for the line status register,
/// which always reports an empty transmitter. Data writes are recorded unless
/// the divisor latch is selected.
#[derive(Debug)]
struct RegisterFileBackend {
    registers: [u8; NUM_REGISTERS],
    sent: Vec<u8>,
}

impl RegisterFileBackend {
    const fn new() -> Self {
        Self {
            registers: [0; NUM_REGISTERS],
            sent: Vec::new(),
        }
    }
}

impl Backend for RegisterFileBackend {
    unsafe fn read(&mut self, register_offset: u8) -> u8 {
        if register_offset == offsets::LSR as u8 {
            return (LSR::THR_EMPTY | LSR::TRANSMITTER_EMPTY).bits();
        }
        self.registers[usize::from(register_offset)]
    }

    unsafe fn write(&mut self, register_offset: u8, value: u8) {
        let dlab = LCR::from_bits_retain(self.registers[offsets::LCR]).contains(LCR::DLAB);
        if register_offset == offsets::DATA as u8 && !dlab {
            self.sent.push(value);
        }
        self.registers[usize::from(register_offset)] = value;
    }
}

/// A custom backend can be used for the full device lifecycle, including a
/// device in a `static`.
#[test]
fn test_custom_backend() {
    static UART: Mutex<Uart16550<RegisterFileBackend>> =
        // SAFETY: The backend operates on an in-memory register file.
        Mutex::new(unsafe { Uart16550::new_with_backend(RegisterFileBackend::new()) });

    let mut uart = UART.lock().unwrap();
    uart.init(Config::default()).unwrap();
    uart.send_bytes_exact(b"hello");
    assert_eq!(uart.backend().sent, b"hello");
}
