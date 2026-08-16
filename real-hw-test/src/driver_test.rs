//! Checks performed through the public `uart_16550` API.
//!
//! These run after raw preflight so a driver failure can be distinguished from
//! an absent or non-responsive UART.

use alloc::vec::Vec;
use core::ptr::NonNull;
use core::time::Duration;

use uart_16550::backend::{MmioBackend, PioBackend};
use uart_16550::spec::registers::{LSR, MCR};
use uart_16550::{BaudRate, Config, ConfigRegisterDump, Uart16550};

use crate::device::{Address, Candidate};
use crate::preflight;
use crate::uefi;
use uefi::boot;

const SEND_TIMEOUT_MS: u64 = 1_000;

/// The public-driver backend selected for a PIO or MMIO candidate.
pub enum Driver {
    Port(Uart16550<PioBackend>),
    Mmio(Uart16550<MmioBackend>),
}

/// The automatic driver result retained for summary and interactive phases.
pub struct Result {
    pub passed: bool,
    pub connection_warning: bool,
    pub interactive_skipped: bool,
    pub driver: Option<Driver>,
}

impl Driver {
    /// Constructs the public backend matching the candidate's address form.
    fn new(address: Address) -> core::result::Result<Self, &'static str> {
        match address {
            Address::Port(port) => {
                // SAFETY: firmware serial consumers were disconnected before candidate discovery.
                unsafe { Uart16550::new_port(port) }
                    .map(Self::Port)
                    .map_err(|_| "invalid PIO address")
            }
            Address::Mmio { base, stride } => {
                let address = NonNull::new(base as *mut u8).ok_or("null MMIO address")?;
                // SAFETY: ACPI/PCI supplied the active MMIO register range and stride.
                unsafe { Uart16550::new_mmio(address, stride) }
                    .map(Self::Mmio)
                    .map_err(|_| "invalid MMIO address or stride")
            }
        }
    }

    /// Initializes either backend with the same configuration for equal coverage.
    fn init(&mut self, config: Config) -> core::result::Result<(), uart_16550::InitError> {
        match self {
            Self::Port(uart) => uart.init(config),
            Self::Mmio(uart) => uart.init(config),
        }
    }

    /// Captures a typed register dump for diagnostics and invariant checks.
    pub fn dump(&mut self) -> ConfigRegisterDump {
        match self {
            Self::Port(uart) => uart.config_register_dump(),
            Self::Mmio(uart) => uart.config_register_dump(),
        }
    }

    /// Exercises the crate's loopback implementation through the chosen backend.
    pub fn test_loopback(&mut self) -> core::result::Result<(), uart_16550::LoopbackError> {
        match self {
            Self::Port(uart) => uart.test_loopback(),
            Self::Mmio(uart) => uart.test_loopback(),
        }
    }

    /// Samples modem-control inputs to diagnose remote cable wiring.
    pub fn check_connected(
        &mut self,
    ) -> core::result::Result<(), uart_16550::RemoteReadyToReceiveError> {
        match self {
            Self::Port(uart) => uart.check_connected(),
            Self::Mmio(uart) => uart.check_connected(),
        }
    }

    /// Delegates the crate's transmitter-readiness check to either backend.
    fn ready_to_send(&mut self) -> core::result::Result<(), uart_16550::ByteSendError> {
        match self {
            Self::Port(uart) => uart.ready_to_send(),
            Self::Mmio(uart) => uart.ready_to_send(),
        }
    }

    /// Sends one byte with the crate's fallible API for explicit coverage.
    fn try_send_byte(&mut self, byte: u8) -> core::result::Result<(), uart_16550::ByteSendError> {
        match self {
            Self::Port(uart) => uart.try_send_byte(byte),
            Self::Mmio(uart) => uart.try_send_byte(byte),
        }
    }

    /// Attempts a slice write and returns the crate's partial-write progress.
    fn send_bytes(&mut self, bytes: &[u8]) -> usize {
        match self {
            Self::Port(uart) => uart.send_bytes(bytes),
            Self::Mmio(uart) => uart.send_bytes(bytes),
        }
    }

    /// Completes a slice write through the crate's synchronous convenience API.
    pub fn send_bytes_exact(&mut self, bytes: &[u8]) {
        match self {
            Self::Port(uart) => uart.send_bytes_exact(bytes),
            Self::Mmio(uart) => uart.send_bytes_exact(bytes),
        }
    }

    /// Polls one received byte so interactive checks never block keyboard input.
    pub fn try_receive_byte(&mut self) -> core::result::Result<u8, uart_16550::ByteReceiveError> {
        match self {
            Self::Port(uart) => uart.try_receive_byte(),
            Self::Mmio(uart) => uart.try_receive_byte(),
        }
    }
}

/// Runs driver checks only after the independent preflight established hardware.
pub fn run(candidates: &[Candidate], preflight: &[preflight::Result]) -> Vec<Result> {
    candidates
        .iter()
        .zip(preflight)
        .map(|(candidate, preflight)| {
            if preflight.passed {
                run_one(candidate)
            } else {
                uefi::println!(
                    "\nSKIP uart_16550 checks for {}: barebones preflight failed",
                    candidate.address
                );
                Result {
                    passed: false,
                    connection_warning: false,
                    interactive_skipped: false,
                    driver: None,
                }
            }
        })
        .collect()
}

/// Exercises init, registers, loopback, modem inputs, and transmit APIs once.
fn run_one(candidate: &Candidate) -> Result {
    uefi::println!("\nuart_16550 checks: {}", candidate.address);
    let mut driver = match Driver::new(candidate.address) {
        Ok(driver) => driver,
        Err(error) => return fail("construct driver", error),
    };
    let config = Config {
        frequency: candidate.clock_hz,
        ..Config::default()
    };

    if let Err(error) = driver.init(config.clone()) {
        uefi::println!("  FAIL: init: {error:?}");
        return failed_driver(driver, false);
    }
    uefi::println!("  PASS: init");

    let dump = driver.dump();
    print_dump("after init", &dump);
    if !valid_dump(&dump, &config) {
        uefi::println!("  FAIL: initialized register values do not match Config");
        return failed_driver(driver, false);
    }
    uefi::println!("  PASS: initialized register values");

    if let Err(error) = driver.test_loopback() {
        uefi::println!("  FAIL: test_loopback: {error:?}");
        return failed_driver(driver, false);
    }
    uefi::println!("  PASS: test_loopback");
    let dump = driver.dump();
    print_dump("after crate loopback", &dump);
    if !valid_dump(&dump, &config) {
        uefi::println!("  FAIL: loopback did not restore configured registers");
        return failed_driver(driver, false);
    }

    let connection_warning = match driver.check_connected() {
        Ok(()) => {
            uefi::println!("  PASS: DSR and CTS report a connected peer");
            false
        }
        Err(error) => {
            uefi::println!("  WARN: connection signals: {error:?}");
            true
        }
    };

    if let Err(error) = exercise_send_apis(&mut driver) {
        uefi::println!("  FAIL: send API checks: {error}");
        print_dump("after send API failure", &driver.dump());
        return failed_driver(driver, connection_warning);
    }
    uefi::println!("  PASS: try_send_byte/send_bytes/send_bytes_exact");

    Result {
        passed: true,
        connection_warning,
        interactive_skipped: false,
        driver: Some(driver),
    }
}

/// Uses every send API in one recognizable payload for remote verification.
fn exercise_send_apis(driver: &mut Driver) -> core::result::Result<(), &'static str> {
    driver.ready_to_send().map_err(|_| "not ready to send")?;
    driver
        .try_send_byte(b'[')
        .map_err(|_| "try_send_byte failed")?;

    send_all_with_timeout(driver, b"send_bytes")?;
    wait_until_ready_to_send(driver)?;
    // Call the convenience API only while THR is empty to keep this test bounded.
    driver.send_bytes_exact(b"]");
    send_all_with_timeout(driver, b" [uart_16550] uart transmit test\r\n")?;
    Ok(())
}

/// Retries the nonblocking send API long enough for a physical UART to drain.
fn send_all_with_timeout(
    driver: &mut Driver,
    bytes: &[u8],
) -> core::result::Result<(), &'static str> {
    let mut remaining = bytes;
    for _ in 0..SEND_TIMEOUT_MS {
        let written = driver.send_bytes(remaining);
        remaining = &remaining[written..];
        if remaining.is_empty() {
            return Ok(());
        }
        boot::stall(Duration::from_millis(1));
    }
    Err("send_bytes timed out")
}

/// Bounds the prerequisite for `send_bytes_exact`, which has no timeout API.
fn wait_until_ready_to_send(driver: &mut Driver) -> core::result::Result<(), &'static str> {
    for _ in 0..SEND_TIMEOUT_MS {
        if driver.ready_to_send().is_ok() {
            return Ok(());
        }
        boot::stall(Duration::from_millis(1));
    }
    Err("transmitter did not become ready")
}

/// Verifies the dump reflects the requested 9600 8N1 polling configuration.
fn valid_dump(dump: &ConfigRegisterDump, config: &Config) -> bool {
    dump.ier.is_empty()
        && dump.lcr.bits() == 0x03
        && dump
            .mcr
            .contains(MCR::DTR | MCR::RTS | MCR::OUT_2_INT_ENABLE)
        && !dump.mcr.contains(MCR::LOOP_BACK)
        && dump.lsr.contains(LSR::THR_EMPTY | LSR::TRANSMITTER_EMPTY)
        && dump.isr.bits() & 0xc0 == 0xc0
        && dump.baud_rate(config) == BaudRate::Baud9600
}

/// Prints every crate-exposed configuration register on one diagnostic line.
pub fn print_dump(label: &str, dump: &ConfigRegisterDump) {
    uefi::println!(
        concat!(
            "  {}: IER={:?} ISR={:?} LCR={:?} MCR={:?} ",
            "LSR={:?} MSR={:?} SPR={:02x} DLL={:02x} DLM={:02x}",
        ),
        label,
        dump.ier,
        dump.isr,
        dump.lcr,
        dump.mcr,
        dump.lsr,
        dump.msr,
        dump.spr,
        dump.dll,
        dump.dlm
    );
}

/// Reports failures that occur before a driver can be retained for diagnostics.
fn fail(stage: &str, error: &str) -> Result {
    uefi::println!("  FAIL: {stage}: {error}");
    Result {
        passed: false,
        connection_warning: false,
        interactive_skipped: false,
        driver: None,
    }
}

/// Retains a constructed driver after failure without allowing interactive use.
fn failed_driver(driver: Driver, connection_warning: bool) -> Result {
    Result {
        passed: false,
        connection_warning,
        interactive_skipped: false,
        driver: Some(driver),
    }
}
