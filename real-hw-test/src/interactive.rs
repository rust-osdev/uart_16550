//! Operator-driven cable, modem-status, reconnect, transmit, and receive checks.
//!
//! Polling keeps interrupts out of scope while a human validates the physical
//! path that deterministic loopback cannot cover.

use core::time::Duration;

use uefi::boot;
use uefi::proto::console::text::{Key, ScanCode};
use uefi::system;

use crate::device::Candidate;
use crate::driver_test::{self, Driver};

/// Offers interactive checks only for UARTs that passed automatic driver tests.
pub fn run(candidates: &[Candidate], results: &mut [driver_test::Result]) {
    uefi::println!("\nInteractive phase (synchronous polling; UART interrupts stay disabled)");
    for (candidate, result) in candidates.iter().zip(results) {
        if !result.passed {
            continue;
        }
        let Some(driver) = result.driver.as_mut() else {
            continue;
        };
        result.interactive_skipped = run_one(candidate, driver);
    }
}

/// Polls one UART while keyboard Escape provides an out-of-band skip control.
fn run_one(candidate: &Candidate, driver: &mut Driver) -> bool {
    uefi::println!("\nInteractive UART: {}", candidate.address);
    uefi::println!("Serial commands: r=registers t=transmit c=connection l=loopback q=next");
    uefi::println!("Other printable ASCII is echoed. Escape locally or over serial skips.");

    loop {
        if local_escape_pressed() {
            uefi::println!("  WARN: interactive checks skipped from local keyboard");
            return true;
        }

        let Ok(byte) = driver.try_receive_byte() else {
            boot::stall(Duration::from_millis(2));
            continue;
        };
        match byte {
            0x1b => {
                uefi::println!("  WARN: interactive checks skipped from serial Escape");
                return true;
            }
            b'r' | b'R' => driver_test::print_dump("interactive", &driver.dump()),
            b't' | b'T' => {
                driver.send_bytes_exact(b"[interactive] uart transmit test\r\n");
                uefi::println!("  transmitted interactive test line");
            }
            b'c' | b'C' => {
                match driver.check_connected() {
                    Ok(()) => uefi::println!("  connection: DSR and CTS asserted"),
                    Err(error) => uefi::println!("  connection warning: {error:?}"),
                }
                driver_test::print_dump("after connection check", &driver.dump());
            }
            b'l' | b'L' => match driver.test_loopback() {
                Ok(()) => uefi::println!("  PASS: interactive loopback"),
                Err(error) => uefi::println!("  FAIL: interactive loopback: {error:?}"),
            },
            b'q' | b'Q' => {
                uefi::println!("  interactive UART complete");
                return false;
            }
            0x20..=0x7e => {
                uefi::println!(
                    "  received ASCII '{}' (0x{byte:02x}); echoing",
                    char::from(byte)
                );
                driver.send_bytes_exact(&[byte]);
            }
            _ => uefi::println!("  received non-printable byte 0x{byte:02x}"),
        }
    }
}

/// Checks Simple Text Input without blocking so serial polling remains responsive.
fn local_escape_pressed() -> bool {
    match system::with_stdin(|input| input.read_key()) {
        Ok(Some(Key::Special(scan_code))) => scan_code == ScanCode::ESCAPE,
        Ok(Some(Key::Printable(key))) => key == '\u{1b}',
        Ok(_) => false,
        Err(error) => {
            uefi::println!("  WARN: local keyboard read failed: {error:?}");
            false
        }
    }
}
