#![no_main]
#![no_std]
#![deny(clippy::undocumented_unsafe_blocks)]

//! Manual UEFI integration test for this repository's `uart_16550` driver.
//!
//! The phases isolate firmware ownership, hardware discovery, and public driver
//! APIs so the screen identifies the failing layer.

extern crate alloc;
extern crate uefi as uefi_rs;

/// The `uefi` crate is bound as `uefi_rs` so that this crate-local `uefi`
/// module can take its name: every file reaches the library through
/// `crate::uefi`, which re-exports the crate but overrides `println!` with
/// `test_println!`, so each diagnostic also lands in the log file. The
/// library's own internal output is unaffected.
mod uefi {
    pub use crate::test_println as println;
    pub use uefi_rs::*;
}

/// Mirrors UEFI diagnostics to the screen and the test-run log file.
#[macro_export]
macro_rules! test_println {
    ($($arg:tt)*) => {
        $crate::logging::println(core::format_args!($($arg)*))
    };
}

mod device;
mod discovery;
mod driver_test;
mod firmware;
mod interactive;
mod logging;

use uefi::prelude::*;

/// The target architecture, recorded in diagnostics and log file names.
#[cfg(target_arch = "aarch64")]
pub const ARCH_NAME: &str = "aarch64";
#[cfg(target_arch = "x86_64")]
pub const ARCH_NAME: &str = "x86_64";
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
compile_error!("unsupported architecture; supported: x86_64, aarch64");

/// Starts the UEFI test and returns success while later commits add phases.
#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("UEFI helpers should initialize");
    if let Err(error) = logging::init() {
        uefi_rs::println!("CRITICAL: cannot create test log: {error}");
        return Status::DEVICE_ERROR;
    }
    uefi::println!("uart_16550 real-hardware test ({ARCH_NAME})");
    firmware::disable_watchdog();

    if !firmware::disconnect_serial_controllers() {
        uefi::println!("FAIL: firmware serial ownership was not released");
        return Status::DEVICE_ERROR;
    }

    let inventory = discovery::discover();
    uefi::println!("\nUsable UART candidates: {}", inventory.candidates().len());
    for (index, candidate) in inventory.candidates().iter().enumerate() {
        uefi::println!(
            "  [{index}] {} clock={} Hz",
            candidate.address,
            candidate.clock_hz
        );
        uefi::println!("      location: {}", candidate.location);
        uefi::println!("      found by: {}", candidate.found_by());
    }

    let mut drivers = driver_test::run(inventory.candidates());
    interactive::run(inventory.candidates(), &mut drivers);
    let passed = drivers.iter().filter(|result| result.passed).count();
    let warnings = drivers
        .iter()
        .filter(|result| result.connection_warning)
        .count();
    let initialized = drivers
        .iter()
        .filter(|result| result.driver.is_some())
        .count();
    let skipped = drivers
        .iter()
        .filter(|result| result.interactive_skipped)
        .count();
    uefi::println!(
        "\nFinal summary: {passed}/{} passed, {warnings} connection warning(s), {skipped} interactive skip(s), {initialized} initialized.",
        drivers.len()
    );
    let statuses = drivers.iter().map(driver_test::Result::status);
    for (index, (candidate, status)) in inventory.candidates().iter().zip(statuses).enumerate() {
        uefi::println!("  [{index}] {status}: {}", candidate.address);
    }
    uefi::println!("Press Enter to return to firmware.");
    firmware::wait_for_enter();
    if passed == drivers.len() {
        Status::SUCCESS
    } else {
        Status::DEVICE_ERROR
    }
}
