//! UART discovery through legacy probing, ACPI SPCR, and PCI enumeration.
//!
//! Multiple discovery paths cover fixed COM ports and dynamically described
//! UARTs, including QEMU's independent PCI serial controller.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use uart_16550::Uart16550;

use crate::device::Inventory;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::device::{Address, Discovery, Location};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::uefi;

mod acpi;
mod pci;

/// Combines every discovery source into a deduplicated test inventory.
pub fn discover() -> Inventory {
    let mut inventory = Inventory::default();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    discover_legacy(&mut inventory);
    acpi::discover(&mut inventory);
    pci::discover(&mut inventory);
    inventory
}

/// Probes conventional COM addresses while always retaining COM1 as a baseline.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn discover_legacy(inventory: &mut Inventory) {
    const PORTS: [u16; 4] = [0x3f8, 0x2f8, 0x3e8, 0x2e8];

    uefi::println!("\nLegacy UART probes:");
    for (index, port) in PORTS.into_iter().enumerate() {
        let address = Address::Port(port);
        let present = uart_present(port);
        uefi::println!(
            "  {address}: presence check {}",
            if present { "PASS" } else { "FAIL" }
        );

        if index == 0 {
            inventory.add(address, None, Discovery::RequiredCom1, Location::LegacyPort);
        } else if present {
            inventory.add(address, None, Discovery::LegacyProbe, Location::LegacyPort);
        }
    }
}

/// Reading an absent port yields junk, so only a responding scratch register
/// qualifies an address; the crate's check is the one `init()` runs first.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn uart_present(port: u16) -> bool {
    // SAFETY: firmware serial consumers were disconnected before discovery.
    unsafe { Uart16550::new_port(port) }.is_ok_and(|mut uart| uart.check_present())
}
