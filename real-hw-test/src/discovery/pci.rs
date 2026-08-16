//! Conservative PCI serial-controller discovery through UEFI root bridges.
//!
//! It verifies BAR-backed PIO/MMIO paths and gives QEMU a device independent of
//! legacy COM1.

use alloc::vec::Vec;

use uefi::Status;
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams};
use uefi::proto::pci::PciIoAddress;
use uefi::proto::pci::root_bridge::PciRootBridgeIo;

use crate::device::{Address, Inventory, Source};

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

    for address in addresses {
        let Ok(class_register) = config_u32(root, address, 0x08) else {
            continue;
        };
        let class = (class_register >> 24) as u8;
        let subclass = (class_register >> 16) as u8;
        if class != 0x07 || subclass != 0x00 {
            continue;
        }

        inspect_serial_controller(root, segment, address, class_register, inventory);
    }
}

/// Validates one endpoint's interface, decoding state, and BAR0 before using it.
fn inspect_serial_controller(
    root: &mut PciRootBridgeIo,
    segment: u32,
    address: PciIoAddress,
    class_register: u32,
    inventory: &mut Inventory,
) {
    let identity = config_u32(root, address, 0x00).unwrap_or(u32::MAX);
    let command = config_u16(root, address, 0x04).unwrap_or(0);
    let header_type = config_u8(root, address, 0x0e).unwrap_or(0xff) & 0x7f;
    let prog_if = (class_register >> 8) as u8;
    let bar0 = config_u32(root, address, 0x10).unwrap_or(0);
    let bar1 = config_u32(root, address, 0x14).unwrap_or(0);
    let vendor = identity as u16;
    let device_id = (identity >> 16) as u16;
    let (bus, device, function) = (address.bus, address.dev, address.fun);

    uefi::println!(
        concat!(
            "  {:04x}:{:02x}:{:02x}.{}: ",
            "{:04x}:{:04x} prog-if=0x{:02x} ",
            "command=0x{:04x} BAR0=0x{:08x}",
        ),
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
    if header_type != 0 || !(0x02..=0x06).contains(&prog_if) {
        uefi::println!("    SKIP: not an unambiguous 16550-compatible endpoint");
        return;
    }

    let candidate = if bar0 & 1 != 0 {
        let base = bar0 & !0x3;
        if command & 1 == 0 || base > u32::from(u16::MAX - 7) {
            None
        } else {
            Some(Address::Port(base as u16))
        }
    } else {
        let memory_type = (bar0 >> 1) & 0x3;
        let base = match memory_type {
            0 => u64::from(bar0 & !0xf),
            2 => (u64::from(bar1) << 32) | u64::from(bar0 & !0xf),
            _ => 0,
        };
        if command & 2 == 0 || base == 0 || base > usize::MAX as u64 {
            None
        } else {
            Some(Address::Mmio {
                base: base as usize,
                stride: 1,
            })
        }
    };

    let Some(candidate) = candidate else {
        uefi::println!("    SKIP: BAR0 is disabled, invalid, or unsupported");
        return;
    };
    uefi::println!("    candidate: {candidate}");
    inventory.add(
        candidate,
        None,
        Source::Pci {
            segment,
            bus,
            device,
            function,
        },
    );
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
