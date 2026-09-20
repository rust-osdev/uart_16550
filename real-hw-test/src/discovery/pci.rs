//! Conservative PCI serial-controller discovery through UEFI root bridges.
//!
//! Conservative means that only endpoints whose class code and programming
//! interface unambiguously declare a 16550-compatible UART with a plain header
//! and an assigned BAR0 are used, and that the only configuration-space write
//! is enabling decoding of that BAR. Vendor-specific layouts are reported but
//! never programmed: guessing a register map on real hardware risks touching
//! an unrelated device. This verifies BAR-backed PIO/MMIO paths and gives QEMU
//! a device independent of legacy COM1.

use alloc::vec::Vec;

use uefi::Status;
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams};
use uefi::proto::pci::PciIoAddress;
use uefi::proto::pci::configuration::ResourceRangeType;
use uefi::proto::pci::root_bridge::PciRootBridgeIo;

use crate::device::{Address, Attachment, Discovery, Inventory, Location, PciFunction};
use crate::uefi;

/// BAR bit 0 selects I/O space; a memory BAR clears it.
const BAR_IO_SPACE: u32 = 1 << 0;
/// An I/O BAR carries its address in bits 2 and up.
const BAR_IO_ADDRESS_MASK: u32 = !0x3;
/// A memory BAR carries its address in bits 4 and up; bits 1-2 give its width.
const BAR_MEMORY_ADDRESS_MASK: u32 = !0xf;
/// Command register bits that enable I/O and memory decoding.
const COMMAND_IO_SPACE: u16 = 1 << 0;
const COMMAND_MEMORY_SPACE: u16 = 1 << 1;

/// Configuration-space registers read for every candidate.
const REG_VENDOR_ID: u8 = 0x00;
const REG_COMMAND: u8 = 0x04;
const REG_CLASS: u8 = 0x08;
const REG_HEADER_TYPE: u8 = 0x0e;
const REG_BAR0: u8 = 0x10;
const REG_BAR1: u8 = 0x14;
/// Header type 0 is a general endpoint; bit 7 only flags a multi-function
/// device.
const HEADER_TYPE_GENERAL: u8 = 0;
const HEADER_TYPE_MASK: u8 = 0x7f;
/// Class 0x07 subclass 0x00 is a serial controller; the programming interface
/// names the UART generation, and 0x02 (16550) through 0x06 (16950) share the
/// 16550 register map.
const CLASS_SIMPLE_COMMUNICATION: u8 = 0x07;
const SUBCLASS_SERIAL: u8 = 0x00;
const PROG_IF_16550: u8 = 0x02;
const PROG_IF_16950: u8 = 0x06;

/// Opens each root bridge read-only and searches it for serial-class endpoints.
pub fn discover(inventory: &mut Inventory) {
    uefi::println!("\nPCI serial-controller discovery:");
    let handles = match boot::find_handles::<PciRootBridgeIo>() {
        Ok(handles) => handles,
        Err(error) if error.status() == Status::NOT_FOUND => {
            uefi::println!("  SKIP: no PCI root bridge protocol");
            return;
        }
        Err(error) => {
            uefi::println!("  SKIP: PCI root bridge lookup failed: {error:?}");
            return;
        }
    };

    for handle in handles {
        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };
        let root = {
            // SAFETY: GetProtocol is read-only and firmware retains the interface.
            unsafe {
                boot::open_protocol::<PciRootBridgeIo>(params, OpenProtocolAttributes::GetProtocol)
            }
        };
        match root {
            Ok(mut root) => discover_root(&mut root, inventory),
            Err(error) => uefi::println!("  root bridge open failed: {error:?}"),
        }
    }
}

/// Enumerates one segment and forwards serial-class functions for BAR inspection.
fn discover_root(root: &mut PciRootBridgeIo, inventory: &mut Inventory) {
    let segment = root.segment_nr();
    let tree = match root.enumerate() {
        Ok(tree) => tree,
        Err(error) => {
            uefi::println!("  segment {segment}: enumeration failed: {error:?}");
            return;
        }
    };
    let addresses: Vec<_> = tree.iter().copied().collect();
    // The bridge's own bus starts its bus range: functions there are integrated
    // controllers, functions on later buses sit behind a bridge.
    let root_bus = root.configuration().ok().and_then(|descriptors| {
        descriptors
            .iter()
            .find(|descriptor| descriptor.resource_range_type == ResourceRangeType::Bus)
            .map(|descriptor| descriptor.address_min as u8)
    });

    for address in addresses {
        let Ok(class_register) = config_u32(root, address, REG_CLASS) else {
            continue;
        };
        let (class, subclass, _) = class_code(class_register);
        if class != CLASS_SIMPLE_COMMUNICATION || subclass != SUBCLASS_SERIAL {
            continue;
        }

        inspect_serial_controller(root, segment, root_bus, address, class_register, inventory);
    }
}

/// Validates one endpoint's interface, decoding state, and BAR0 before using it.
fn inspect_serial_controller(
    root: &mut PciRootBridgeIo,
    segment: u32,
    root_bus: Option<u8>,
    address: PciIoAddress,
    class_register: u32,
    inventory: &mut Inventory,
) {
    let identity = config_u32(root, address, REG_VENDOR_ID).unwrap_or(u32::MAX);
    let command = config_u16(root, address, REG_COMMAND).unwrap_or(0);
    let header_type = config_u8(root, address, REG_HEADER_TYPE).unwrap_or(0xff) & HEADER_TYPE_MASK;
    let (_, _, prog_if) = class_code(class_register);
    let bar0 = config_u32(root, address, REG_BAR0).unwrap_or(0);
    let bar1 = config_u32(root, address, REG_BAR1).unwrap_or(0);
    let vendor = identity as u16;
    let device_id = (identity >> 16) as u16;
    let (bus, device, function) = (address.bus, address.dev, address.fun);

    uefi::println!(
        "  {:04x}:{:02x}:{:02x}.{}: \
         {:04x}:{:04x} prog-if=0x{:02x} \
         command=0x{:04x} BAR0=0x{:08x}",
        segment,
        bus,
        device,
        function,
        vendor,
        device_id,
        prog_if,
        command,
        bar0,
    );
    if header_type != HEADER_TYPE_GENERAL || !(PROG_IF_16550..=PROG_IF_16950).contains(&prog_if) {
        uefi::println!("    SKIP: not an unambiguous 16550-compatible endpoint");
        return;
    }

    // Firmware enables decoding only for endpoints it binds a driver to; an
    // otherwise valid UART may therefore arrive with its assigned BAR disabled.
    let needed_enable = if bar0 & BAR_IO_SPACE != 0 {
        COMMAND_IO_SPACE
    } else {
        COMMAND_MEMORY_SPACE
    };
    let command = if command & needed_enable == 0 {
        match enable_decoding(root, address, command | needed_enable) {
            Some(command) => command,
            None => {
                uefi::println!("    SKIP: could not enable BAR0 decoding");
                return;
            }
        }
    } else {
        command
    };

    let candidate = if bar0 & BAR_IO_SPACE != 0 {
        io_bar_candidate(root, bar0, command)
    } else {
        memory_bar_candidate(bar0, bar1, command)
    };

    let Some(candidate) = candidate else {
        uefi::println!("    SKIP: BAR0 is disabled, invalid, or unsupported");
        return;
    };
    let attachment = root_bus.map(|root_bus| {
        if bus == root_bus {
            Attachment::OnRootBus
        } else {
            Attachment::BehindBridge
        }
    });
    let location = Location::Pci(PciFunction {
        segment,
        bus,
        device,
        function,
        vendor_id: vendor,
        device_id,
        attachment,
    });
    uefi::println!("    candidate: {candidate} ({location})");
    inventory.add(candidate, None, Discovery::PciEnumeration, location);
}

/// Uses an I/O BAR whose decoding is enabled, reaching it as the architecture
/// allows.
fn io_bar_candidate(root: &mut PciRootBridgeIo, bar0: u32, command: u16) -> Option<Address> {
    if command & COMMAND_IO_SPACE == 0 {
        return None;
    }
    io_bar_address(root, bar0 & BAR_IO_ADDRESS_MASK)
}

/// Uses a 32- or 64-bit memory BAR whose decoding is enabled and whose address
/// fits the platform.
fn memory_bar_candidate(bar0: u32, bar1: u32, command: u16) -> Option<Address> {
    // Bits 1-2 encode the BAR width: 0 is 32-bit, 2 is 64-bit with the high
    // half in the next BAR.
    let base = match (bar0 >> 1) & 0x3 {
        0 => u64::from(bar0 & BAR_MEMORY_ADDRESS_MASK),
        2 => (u64::from(bar1) << 32) | u64::from(bar0 & BAR_MEMORY_ADDRESS_MASK),
        _ => return None,
    };
    if command & COMMAND_MEMORY_SPACE == 0 || base == 0 || base > usize::MAX as u64 {
        return None;
    }
    Some(Address::Mmio {
        base: base as usize,
        stride: 1,
    })
}

/// Sets a missing decode-enable bit and returns the verified command register.
fn enable_decoding(root: &mut PciRootBridgeIo, address: PciIoAddress, command: u16) -> Option<u16> {
    root.pci()
        .write_one(address.with_register(REG_COMMAND), command)
        .ok()?;
    let command = config_u16(root, address, REG_COMMAND).ok()?;
    uefi::println!("    enabled BAR0 decoding: command=0x{command:04x}");
    Some(command)
}

/// Uses an I/O BAR directly: x86 port instructions reach PCI I/O space as-is.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn io_bar_address(_root: &mut PciRootBridgeIo, base: u32) -> Option<Address> {
    (base <= u32::from(u16::MAX - 7)).then_some(Address::Port(base as u16))
}

/// Translates an I/O BAR into the platform's memory-mapped I/O window.
///
/// Without port instructions, PCI I/O space is reached through an MMIO
/// aperture. Firmware hides its CPU-side base inside the root bridge protocol,
/// so the window is taken from the platform's ACPI description instead.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn io_bar_address(_root: &mut PciRootBridgeIo, base: u32) -> Option<Address> {
    let Some(window) = super::acpi::pci_io_window() else {
        uefi::println!("    SKIP: no unambiguous ACPI PCI I/O window");
        return None;
    };
    let base = u64::from(base);
    if base < window.pci_min || base + 7 > window.pci_max {
        uefi::println!("    SKIP: I/O BAR lies outside the ACPI I/O window");
        return None;
    }
    let translated = base
        .checked_sub(window.pci_min)?
        .checked_add(window.cpu_base)?;
    uefi::println!("    I/O window translation: 0x{base:x} -> 0x{translated:x}");
    Some(Address::Mmio {
        base: usize::try_from(translated).ok()?,
        stride: 1,
    })
}

/// Splits the class register into class, subclass, and programming interface.
fn class_code(register: u32) -> (u8, u8, u8) {
    (
        (register >> 24) as u8,
        (register >> 16) as u8,
        (register >> 8) as u8,
    )
}

/// Reads one byte from PCI configuration space through the root bridge.
fn config_u8(root: &mut PciRootBridgeIo, address: PciIoAddress, offset: u8) -> uefi::Result<u8> {
    root.pci().read_one(address.with_register(offset))
}

/// Reads one 16-bit PCI configuration value through the root bridge.
fn config_u16(root: &mut PciRootBridgeIo, address: PciIoAddress, offset: u8) -> uefi::Result<u16> {
    root.pci().read_one(address.with_register(offset))
}

/// Reads one 32-bit PCI configuration value through the root bridge.
fn config_u32(root: &mut PciRootBridgeIo, address: PciIoAddress, offset: u8) -> uefi::Result<u32> {
    root.pci().read_one(address.with_register(offset))
}
