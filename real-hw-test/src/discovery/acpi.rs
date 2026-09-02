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
    let Some(rsdp) = rsdp() else {
        uefi::println!("  SKIP: no ACPI RSDP in the UEFI configuration table");
        return;
    };

    match find_spcr(rsdp) {
        Ok(Some(spcr)) => add_spcr(inventory, spcr),
        Ok(None) => uefi::println!("  SKIP: no SPCR table"),
        Err(reason) => uefi::println!("  SKIP: invalid ACPI data: {reason}"),
    }
}

/// Returns the RSDP address from the UEFI configuration table, preferring ACPI 2.
fn rsdp() -> Option<usize> {
    system::with_config_table(|tables| {
        tables
            .iter()
            .find(|entry| entry.guid == ConfigTableEntry::ACPI2_GUID)
            .or_else(|| {
                tables
                    .iter()
                    .find(|entry| entry.guid == ConfigTableEntry::ACPI_GUID)
            })
            .map(|entry| entry.address as usize)
    })
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
        0 => {
            uefi::println!("  SKIP: SPCR base address is out of range");
            return;
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        1 if spcr.base <= u64::from(u16::MAX - 7) => Address::Port(spcr.base as u16),
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        1 => {
            uefi::println!("  SKIP: SPCR base address is out of range");
            return;
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        1 => {
            uefi::println!("  SKIP: System I/O access requires x86 port instructions");
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

/// Finds and decodes an SPCR table, requiring the fields this test consumes.
fn find_spcr(rsdp_address: usize) -> Result<Option<SpcrInfo>, &'static str> {
    let Some(table) = find_table(rsdp_address, b"SPCR")? else {
        return Ok(None);
    };
    if table.len() < 80 {
        return Err("SPCR is too short");
    }
    let clock = read_u32(table, 76);
    Ok(Some(SpcrInfo {
        interface: table[36],
        address_space: table[40],
        bit_width: table[41],
        bit_offset: table[42],
        access_size: table[43],
        base: read_u64(table, 44),
        clock_hz: (clock != 0).then_some(clock),
    }))
}

/// Validates RSDP and XSDT/RSDT data before returning one table by signature.
fn find_table(
    rsdp_address: usize,
    signature: &[u8; 4],
) -> Result<Option<&'static [u8]>, &'static str> {
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
        if &header[..4] != signature {
            continue;
        }
        return sdt(address).map(Some);
    }
    Ok(None)
}

/// The CPU-visible MMIO window ACPI declares for the PCI I/O address space.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IoWindow {
    pub pci_min: u64,
    pub pci_max: u64,
    pub cpu_base: u64,
}

/// Recovers the PCI I/O window translation from the DSDT's resource bytes.
///
/// AML resource templates embed plain ACPI address-space descriptors, so a
/// strictly validated byte scan finds the root bridge's translated I/O range
/// without an AML interpreter. Ambiguous DSDTs yield no window.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn pci_io_window() -> Option<IoWindow> {
    let fadt = find_table(rsdp()?, b"FACP").ok().flatten()?;
    let dsdt_address = if fadt.len() >= 148 && read_u64(fadt, 140) != 0 {
        read_u64(fadt, 140) as usize
    } else if fadt.len() >= 44 {
        read_u32(fadt, 40) as usize
    } else {
        return None;
    };
    let dsdt = sdt(dsdt_address).ok()?;

    let mut found: Option<IoWindow> = None;
    let mut offset = 0;
    while offset < dsdt.len() {
        let (window, size) = match parse_io_descriptor(&dsdt[offset..]) {
            Some(parsed) => parsed,
            None => {
                offset += 1;
                continue;
            }
        };
        offset += size;
        match found {
            None => found = Some(window),
            Some(previous) if previous == window => {}
            Some(_) => return None,
        }
    }
    found
}

/// Decodes one translated DWord/QWord I/O descriptor at the slice's start.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn parse_io_descriptor(bytes: &[u8]) -> Option<(IoWindow, usize)> {
    const DWORD_IO: (u8, u16, usize) = (0x87, 23, 4);
    const QWORD_IO: (u8, u16, usize) = (0x8a, 43, 8);
    const TYPE_IO: u8 = 1;

    let (_, body_len, field_size) = [DWORD_IO, QWORD_IO]
        .into_iter()
        .find(|(tag, _, _)| bytes.first() == Some(tag))?;
    let size = 3 + usize::from(body_len);
    if bytes.len() < size
        || u16::from_le_bytes([bytes[1], bytes[2]]) != body_len
        || bytes[3] != TYPE_IO
    {
        return None;
    }
    let field = |index: usize| {
        let offset = 6 + index * field_size;
        if field_size == 8 {
            read_u64(bytes, offset)
        } else {
            u64::from(read_u32(bytes, offset))
        }
    };
    let (pci_min, pci_max, translation, length) = (field(1), field(2), field(3), field(4));

    // Only an arithmetically consistent, actually translated window is usable.
    let consistent = pci_min <= pci_max
        && length == pci_max - pci_min + 1
        && translation != 0
        && translation.checked_add(pci_max).is_some();
    consistent.then_some((
        IoWindow {
            pci_min,
            pci_max,
            cpu_base: pci_min + translation,
        },
        size,
    ))
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
