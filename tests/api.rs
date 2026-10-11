// SPDX-License-Identifier: MIT OR Apache-2.0

use core::fmt::Write;
use core::num::NonZeroUsize;
use core::ptr::{self, NonNull};
use std::sync::Mutex;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use uart_16550::backend::PioBackend;
use uart_16550::backend::{Backend, MmioBackend};
use uart_16550::spec::NUM_REGISTERS;
use uart_16550::spec::registers::{FCR, IER, ISR, LCR, LSR, offsets};
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
/// which always reports an empty transmitter, and the interrupt status
/// register, which reports the FIFO state. Data writes are recorded unless the
/// divisor latch is selected.
#[derive(Debug)]
struct RegisterFileBackend {
    registers: [u8; NUM_REGISTERS],
    sent: Vec<u8>,
    /// Whether the device has FIFOs. Without, it behaves like a 16450.
    has_fifo: bool,
}

impl RegisterFileBackend {
    const fn new() -> Self {
        Self {
            registers: [0; NUM_REGISTERS],
            sent: Vec::new(),
            has_fifo: true,
        }
    }

    fn without_fifo() -> Self {
        Self {
            has_fifo: false,
            ..Self::new()
        }
    }
}

impl Backend for RegisterFileBackend {
    unsafe fn read(&mut self, register_offset: u8) -> u8 {
        if register_offset == offsets::LSR as u8 {
            return (LSR::THR_EMPTY | LSR::TRANSMITTER_EMPTY).bits();
        }
        if register_offset == offsets::ISR as u8 {
            // The register file holds the last FCR write at this offset.
            let fifo_enabled =
                FCR::from_bits_retain(self.registers[offsets::FCR]).contains(FCR::FIFO_ENABLE);
            return if self.has_fifo && fifo_enabled {
                (ISR::INTERRUPT_STATUS | ISR::FIFOS_ENABLED0 | ISR::FIFOS_ENABLED1).bits()
            } else {
                ISR::INTERRUPT_STATUS.bits()
            };
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

/// The driver detects whether the FIFOs are really enabled, and sends one byte
/// at a time without them, e.g., on a 16450.
#[test]
fn test_fifo_detection() {
    // SAFETY: The backend operates on an in-memory register file.
    let mut uart = unsafe { Uart16550::new_with_backend(RegisterFileBackend::new()) };
    uart.init(Config::default()).unwrap();
    assert!(uart.fifo_enabled());
    assert_eq!(uart.send_bytes(b"hello"), 5);

    // SAFETY: The backend operates on an in-memory register file.
    let mut uart = unsafe { Uart16550::new_with_backend(RegisterFileBackend::without_fifo()) };
    uart.init(Config::default()).unwrap();
    assert!(!uart.fifo_enabled());
    assert_eq!(uart.send_bytes(b"hello"), 1);
}

/// Interrupts can be enabled and disabled after `init()`.
#[test]
fn test_enable_disable_interrupts() {
    // SAFETY: The backend operates on an in-memory register file.
    let mut uart = unsafe { Uart16550::new_with_backend(RegisterFileBackend::new()) };
    uart.init(Config {
        interrupts: IER::DATA_READY,
        ..Config::default()
    })
    .unwrap();

    uart.enable_interrupts(IER::THR_EMPTY);
    assert_eq!(uart.ier(), IER::DATA_READY | IER::THR_EMPTY);
    assert_eq!(uart.config().interrupts, IER::DATA_READY | IER::THR_EMPTY);
    uart.disable_interrupts(IER::DATA_READY);
    assert_eq!(uart.ier(), IER::THR_EMPTY);
    assert_eq!(uart.config().interrupts, IER::THR_EMPTY);
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

/// A [`Uart16550Tty`] can wrap a device in a `static` without accessing it.
#[test]
fn test_custom_backend_tty() {
    static TTY: Mutex<Uart16550Tty<RegisterFileBackend>> = Mutex::new(Uart16550Tty::from_inner(
        // SAFETY: The backend operates on an in-memory register file.
        unsafe { Uart16550::new_with_backend(RegisterFileBackend::new()) },
    ));

    let mut tty = TTY.lock().unwrap();
    assert!(tty.inner().backend().sent.is_empty());
    writeln!(tty, "hello").unwrap();
    assert_eq!(tty.inner().backend().sent, b"hello\r\n");
}
