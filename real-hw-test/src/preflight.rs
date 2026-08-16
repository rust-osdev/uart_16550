//! Barebones register-level UART checks before the crate driver is constructed.
//!
//! This independent path validates addresses, clocks, FIFOs, and loopback so a
//! later public-API failure is easier to diagnose.

use alloc::vec::Vec;

use crate::device::Candidate;
use crate::raw_uart::RawUart;

#[derive(Clone, Copy, Debug)]
/// The automatic raw preflight outcome used to gate driver construction.
pub struct Result {
    pub passed: bool,
}

/// Runs the independent register-level preflight for every discovered UART.
pub fn run(candidates: &[Candidate]) -> Vec<Result> {
    candidates.iter().map(run_one).collect()
}

/// Initializes, snapshots, loopbacks, validates, and transmits on one UART.
fn run_one(candidate: &Candidate) -> Result {
    uefi::println!("\nBarebones preflight: {}", candidate.address);
    let mut uart = RawUart::new(candidate.address);
    uart.snapshot().print("initial");

    let divisor = match uart.initialize(candidate.clock_hz) {
        Ok(divisor) => {
            uefi::println!("  PASS: initialized at 9600 8N1 (divisor {divisor})");
            divisor
        }
        Err(error) => return fail("initialization", error),
    };
    uart.snapshot().print("after raw init");

    if let Err(error) = uart.test_loopback() {
        return fail("single-byte/FIFO loopback", error);
    }
    uefi::println!("  PASS: single-byte and 16-byte loopback");
    uart.snapshot().print("after raw loopback");

    if let Err(error) = uart.validate_configuration(divisor) {
        return fail("register validation", error);
    }
    uefi::println!("  PASS: register invariants");

    if let Err(error) = uart.send_bytes(b"[barebones] uart transmit test\r\n") {
        return fail("transmit payload", error);
    }
    uefi::println!("  PASS: barebones transmit payload queued");
    Result { passed: true }
}

/// Prints a consistently labelled raw-preflight failure result.
fn fail(stage: &str, error: crate::raw_uart::PreflightError) -> Result {
    uefi::println!("  FAIL: {stage}: {error:?}");
    Result { passed: false }
}
