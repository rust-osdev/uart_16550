//! Conservative ACPI SPCR discovery for firmware-described serial consoles.
//!
//! SPCR matters where a debug UART is not at a conventional COM address. Strict
//! validation prevents treating an incompatible layout as a 16550 device.

use core::slice;

use uefi::system;
use uefi::table::cfg::ConfigTableEntry;

use crate::device::{Address, Inventory, Source};
use crate::uefi;

const SDT_HEADER_LEN: usize = 36;
const MAX_TABLE_LEN: usize = 1024 * 1024;

/// Locates SPCR from UEFI configuration tables and safely skips invalid data.
pub fn discover(inventory: &mut Inventory) {
    uefi::println!("\nACPI SPCR discovery:");
    let rsdp = system::with_config_table(|tables| {
        tables
            .iter()
            .find(|entry| entry.guid == ConfigTableEntry::ACPI2_GUID)
            .or_else(|| {
                tables
                    .iter()
                    .find(|entry| entry.guid == ConfigTableEntry::ACPI_GUID)
            })
            .map(|entry| entry.address as usize)
    });

    let Some(rsdp) = rsdp else {
        uefi::println!("  SKIP: no ACPI RSDP in the UEFI configuration table");
        return;
    };

    match find_spcr(rsdp) {
        Ok(Some(spcr)) => add_spcr(inventory, spcr),
        Ok(None) => uefi::println!("  SKIP: no SPCR table"),
        Err(reason) => uefi::println!("  SKIP: invalid ACPI data: {reason}"),
    }
}

/// The SPCR subset needed to validate and add a byte-access UART candidate.
#[derive(Clone, Copy)]
struct SpcrInfo {
    interface: u8,
    address_space: u8,
    bit_width: u8,
    bit_offset: u8,
    access_size: u8,
    base: u64,
    clock_hz: Option<u32>,
}

/// Accepts only SPCR layouts that the byte-oriented driver can safely access.
fn add_spcr(inventory: &mut Inventory, spcr: SpcrInfo) {
    uefi::println!(
        concat!(
            "  interface=0x{:02x} space={} base=0x{:x} width={} ",
            "offset={} access={} clock={:?}",
        ),
        spcr.interface,
        spcr.address_space,
        spcr.base,
        spcr.bit_width,
        spcr.bit_offset,
        spcr.access_size,
        spcr.clock_hz
    );

    if !matches!(spcr.interface, 0x00 | 0x01 | 0x12) {
        uefi::println!("  SKIP: SPCR interface is not 16450/16550-compatible");
        return;
    }
    if spcr.bit_offset != 0 || !matches!(spcr.bit_width, 0 | 8) {
        uefi::println!("  SKIP: UART registers are not byte-aligned byte fields");
        return;
    }
    if !matches!(spcr.access_size, 0 | 1) {
        uefi::println!("  SKIP: uart_16550 requires byte register accesses");
        return;
    }

    let address = match spcr.address_space {
        0 if spcr.base <= (usize::MAX - 7) as u64 => Address::Mmio {
            base: spcr.base as usize,
            stride: 1,
        },
        1 if spcr.base <= u64::from(u16::MAX - 7) => Address::Port(spcr.base as u16),
        0 | 1 => {
            uefi::println!("  SKIP: SPCR base address is out of range");
            return;
        }
        _ => {
            uefi::println!("  SKIP: unsupported ACPI address space");
            return;
        }
    };

    uefi::println!("  candidate: {address}");
    inventory.add(address, spcr.clock_hz, Source::AcpiSpcr);
}

/// Validates RSDP and XSDT/RSDT data before finding and decoding an SPCR table.
fn find_spcr(rsdp_address: usize) -> Result<Option<SpcrInfo>, &'static str> {
    let rsdp = acpi_bytes(rsdp_address, 36)?;
    if &rsdp[..8] != b"RSD PTR " || !checksum_ok(&rsdp[..20]) {
        return Err("bad RSDP signature or checksum");
    }

    let revision = rsdp[15];
    let (root_address, entry_size) = if revision >= 2 {
        let length = read_u32(rsdp, 20) as usize;
        if !(36..=4096).contains(&length) {
            return Err("invalid RSDP length");
        }
        let full = acpi_bytes(rsdp_address, length)?;
        if !checksum_ok(full) {
            return Err("bad extended RSDP checksum");
        }
        (read_u64(full, 24) as usize, 8)
    } else {
        (read_u32(rsdp, 16) as usize, 4)
    };

    let root = sdt(root_address)?;
    let expected = if entry_size == 8 { b"XSDT" } else { b"RSDT" };
    if &root[..4] != expected {
        return Err("root table has the wrong signature");
    }

    for entry in root[SDT_HEADER_LEN..].chunks_exact(entry_size) {
        let address = if entry_size == 8 {
            read_u64(entry, 0) as usize
        } else {
            read_u32(entry, 0) as usize
        };
        let header = acpi_bytes(address, SDT_HEADER_LEN)?;
        if &header[..4] != b"SPCR" {
            continue;
        }
        let table = sdt(address)?;
        if table.len() < 80 {
            return Err("SPCR is too short");
        }
        let clock = read_u32(table, 76);
        return Ok(Some(SpcrInfo {
            interface: table[36],
            address_space: table[40],
            bit_width: table[41],
            bit_offset: table[42],
            access_size: table[43],
            base: read_u64(table, 44),
            clock_hz: (clock != 0).then_some(clock),
        }));
    }
    Ok(None)
}

/// Borrows mapped firmware ACPI memory after rejecting a null physical address.
fn acpi_bytes(address: usize, length: usize) -> Result<&'static [u8], &'static str> {
    if address == 0 {
        return Err("null ACPI table address");
    }
    // SAFETY: UEFI keeps firmware ACPI memory mapped while boot services run.
    Ok(unsafe { slice::from_raw_parts(address as *const u8, length) })
}

/// Validates an SDT's declared bounded length and complete ACPI checksum.
fn sdt(address: usize) -> Result<&'static [u8], &'static str> {
    let header = acpi_bytes(address, SDT_HEADER_LEN)?;
    let length = read_u32(header, 4) as usize;
    if !(SDT_HEADER_LEN..=MAX_TABLE_LEN).contains(&length) {
        return Err("invalid SDT length");
    }
    let table = acpi_bytes(address, length)?;
    checksum_ok(table)
        .then_some(table)
        .ok_or("bad SDT checksum")
}

/// Applies ACPI's wrapping-byte checksum rule to one complete table region.
fn checksum_ok(bytes: &[u8]) -> bool {
    bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

/// Decodes a bounds-checked little-endian 32-bit ACPI field without raw offsets.
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let value = bytes[offset..offset + 4]
        .try_into()
        .expect("caller validated ACPI field bounds");
    u32::from_le_bytes(value)
}

/// Decodes a bounds-checked little-endian 64-bit ACPI field without raw offsets.
fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    let value = bytes[offset..offset + 8]
        .try_into()
        .expect("caller validated ACPI field bounds");
    u64::from_le_bytes(value)
}
