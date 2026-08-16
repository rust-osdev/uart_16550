#![no_main]
#![no_std]
#![deny(clippy::undocumented_unsafe_blocks)]

//! Manual UEFI integration test for this repository's `uart_16550` driver.
//!
//! The phases isolate firmware ownership, hardware discovery, and public driver
//! APIs so the screen identifies the failing layer.

extern crate alloc;
extern crate uefi as uefi_rs;

/// Routes every UEFI diagnostic through one crate-local indirection point.
mod uefi {
    pub use uefi_rs::*;
}

mod device;
mod discovery;
mod firmware;

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
    uefi::println!("uart_16550 real-hardware test ({ARCH_NAME})");

    if !firmware::disconnect_serial_controllers() {
        uefi::println!("FAIL: firmware serial ownership was not released");
        return Status::DEVICE_ERROR;
    }

    let inventory = discovery::discover();
    uefi::println!("\nUsable UART candidates: {}", inventory.candidates().len());
    for (index, candidate) in inventory.candidates().iter().enumerate() {
        uefi::println!(
            "  [{index}] {} clock={} Hz sources={:?}",
            candidate.address,
            candidate.clock_hz,
            candidate.sources
        );
    }
    uefi::println!("\nDiscovery complete. Press Enter to return to firmware.");
    firmware::wait_for_enter();
    Status::SUCCESS
}
