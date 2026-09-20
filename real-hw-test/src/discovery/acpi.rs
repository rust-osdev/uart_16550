//! Conservative ACPI SPCR discovery for firmware-described serial consoles.
//!
//! In practice SPCR matters on three kinds of machines: x86 servers with
//! console redirection enabled, where it names the COM port firmware uses
//! (usually the port the legacy probe finds anyway); Arm servers, where it is
//! the only standardized description of the console but almost always names
//! a PL011 or SBSA UART that is rejected here; and SoCs whose firmware
//! describes a byte-access 16550, for which it is the only discovery path.
//! Client x86 machines rarely publish the table at all. Strict validation
//! prevents treating an incompatible layout as a 16550 device; only ACPI 2.0
//! tables (XSDT) are read, which every UEFI machine provides.
//!
//! Without port I/O instructions the module also recovers the PCI I/O window
//! from the DSDT, see `pci_io_window`.

use core::slice;

use uefi::system;
use uefi::table::cfg::ConfigTableEntry;

use crate::device::{Address, Discovery, Inventory, Location, PciFunction};
use crate::uefi;

const SDT_HEADER_LEN: usize = 36;
const MAX_TABLE_LEN: usize = 1024 * 1024;
/// Every ACPI table starts with the same header: signature, length, revision.
const SDT_SIGNATURE_LEN: usize = 4;
const SDT_LENGTH_OFFSET: usize = 4;
const SDT_REVISION_OFFSET: usize = 8;

// RSDP layout: the ACPI 1.0 part is 20 bytes; ACPI 2.0 appends the length,
// the XSDT address, and an extended checksum.
const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";
const RSDP_REVISION_OFFSET: usize = 15;
const RSDP_REVISION_ACPI2: u8 = 2;
const RSDP_V1_LEN: usize = 20;
const RSDP_LENGTH_OFFSET: usize = 20;
const RSDP_XSDT_ADDRESS_OFFSET: usize = 24;
const RSDP_V2_MIN_LEN: usize = 36;
const RSDP_MAX_LEN: usize = 4096;

// SPCR layout: the interface type, the console's Generic Address Structure,
// the PCI identity that revision 2 added, and the clock that revision 3 added.
const SPCR_MIN_LEN: usize = 80;
const SPCR_INTERFACE_OFFSET: usize = 36;
const SPCR_GAS_OFFSET: usize = 40;
const SPCR_PCI_DEVICE_ID_OFFSET: usize = 64;
const SPCR_PCI_VENDOR_ID_OFFSET: usize = 66;
const SPCR_PCI_BUS_OFFSET: usize = 68;
const SPCR_PCI_DEVICE_OFFSET: usize = 69;
const SPCR_PCI_FUNCTION_OFFSET: usize = 70;
const SPCR_PCI_SEGMENT_OFFSET: usize = 75;
const SPCR_CLOCK_OFFSET: usize = 76;
const SPCR_REVISION_WITH_PCI_IDENTITY: u8 = 2;
const SPCR_REVISION_WITH_CLOCK: u8 = 3;
/// Vendor or device ID value marking a console that is not a PCI function.
const PCI_ID_NONE: u16 = 0xffff;

// Generic Address Structure fields, relative to the structure's start.
const GAS_ADDRESS_SPACE: usize = 0;
const GAS_BIT_WIDTH: usize = 1;
const GAS_BIT_OFFSET: usize = 2;
const GAS_ACCESS_SIZE: usize = 3;
const GAS_ADDRESS: usize = 4;

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

/// Returns the ACPI 2.0+ RSDP address from the UEFI configuration table.
fn rsdp() -> Option<usize> {
    system::with_config_table(|tables| {
        tables
            .iter()
            .find(|entry| entry.guid == ConfigTableEntry::ACPI2_GUID)
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
    pci: Option<SpcrPci>,
}

/// The PCI identity SPCR carries when the console UART is a PCI function.
#[derive(Clone, Copy)]
struct SpcrPci {
    segment: u8,
    bus: u8,
    device: u8,
    function: u8,
    vendor_id: u16,
    device_id: u16,
}

/// Accepts only SPCR layouts that the byte-oriented driver can safely access.
fn add_spcr(inventory: &mut Inventory, spcr: SpcrInfo) {
    uefi::println!(
        "  interface=0x{:02x} space={} base=0x{:x} width={} \
         offset={} access={} clock={:?}",
        spcr.interface,
        spcr.address_space,
        spcr.base,
        spcr.bit_width,
        spcr.bit_offset,
        spcr.access_size,
        spcr.clock_hz
    );
    if let Some(pci) = spcr.pci {
        uefi::println!(
            "  PCI identity: {:04x}:{:02x}:{:02x}.{} {:04x}:{:04x}",
            pci.segment,
            pci.bus,
            pci.device,
            pci.function,
            pci.vendor_id,
            pci.device_id
        );
    }

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

    let location = match spcr.pci {
        Some(pci) => Location::Pci(PciFunction {
            segment: u32::from(pci.segment),
            bus: pci.bus,
            device: pci.device,
            function: pci.function,
            vendor_id: pci.vendor_id,
            device_id: pci.device_id,
            attachment: None,
        }),
        None => match address {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Address::Port(_) => Location::LegacyPort,
            Address::Mmio { .. } => Location::Platform,
        },
    };
    uefi::println!("  candidate: {address} ({location})");
    inventory.add(address, spcr.clock_hz, Discovery::AcpiSpcr, location);
}

/// Finds and decodes an SPCR table, requiring the fields this test consumes.
fn find_spcr(rsdp_address: usize) -> Result<Option<SpcrInfo>, &'static str> {
    let Some(table) = find_table(rsdp_address, b"SPCR")? else {
        return Ok(None);
    };
    if table.len() < SPCR_MIN_LEN {
        return Err("SPCR is too short");
    }
    // Revision 3 added the clock field; older tables keep reserved bytes there.
    let clock = if table[SDT_REVISION_OFFSET] >= SPCR_REVISION_WITH_CLOCK {
        read_u32(table, SPCR_CLOCK_OFFSET)
    } else {
        0
    };
    let device_id = read_u16(table, SPCR_PCI_DEVICE_ID_OFFSET);
    let vendor_id = read_u16(table, SPCR_PCI_VENDOR_ID_OFFSET);
    // The PCI identity is only trusted on revision 2 and newer tables.
    let pci = (table[SDT_REVISION_OFFSET] >= SPCR_REVISION_WITH_PCI_IDENTITY
        && device_id != PCI_ID_NONE
        && vendor_id != PCI_ID_NONE)
        .then(|| SpcrPci {
            segment: table[SPCR_PCI_SEGMENT_OFFSET],
            bus: table[SPCR_PCI_BUS_OFFSET],
            device: table[SPCR_PCI_DEVICE_OFFSET],
            function: table[SPCR_PCI_FUNCTION_OFFSET],
            vendor_id,
            device_id,
        });
    Ok(Some(SpcrInfo {
        interface: table[SPCR_INTERFACE_OFFSET],
        address_space: table[SPCR_GAS_OFFSET + GAS_ADDRESS_SPACE],
        bit_width: table[SPCR_GAS_OFFSET + GAS_BIT_WIDTH],
        bit_offset: table[SPCR_GAS_OFFSET + GAS_BIT_OFFSET],
        access_size: table[SPCR_GAS_OFFSET + GAS_ACCESS_SIZE],
        base: read_u64(table, SPCR_GAS_OFFSET + GAS_ADDRESS),
        clock_hz: (clock != 0).then_some(clock),
        pci,
    }))
}

/// Validates RSDP and XSDT/RSDT data before returning one table by signature.
fn find_table(
    rsdp_address: usize,
    signature: &[u8; 4],
) -> Result<Option<&'static [u8]>, &'static str> {
    let rsdp = acpi_bytes(rsdp_address, RSDP_V2_MIN_LEN)?;
    if &rsdp[..RSDP_SIGNATURE.len()] != RSDP_SIGNATURE || !checksum_ok(&rsdp[..RSDP_V1_LEN]) {
        return Err("bad RSDP signature or checksum");
    }

    // ACPI 2.0 and newer only: every UEFI machine provides an XSDT.
    if rsdp[RSDP_REVISION_OFFSET] < RSDP_REVISION_ACPI2 {
        return Err("ACPI 1.0 RSDP without an XSDT");
    }
    let length = read_u32(rsdp, RSDP_LENGTH_OFFSET) as usize;
    if !(RSDP_V2_MIN_LEN..=RSDP_MAX_LEN).contains(&length) {
        return Err("invalid RSDP length");
    }
    let full = acpi_bytes(rsdp_address, length)?;
    if !checksum_ok(full) {
        return Err("bad extended RSDP checksum");
    }

    let root = sdt(read_u64(full, RSDP_XSDT_ADDRESS_OFFSET) as usize)?;
    if &root[..SDT_SIGNATURE_LEN] != b"XSDT" {
        return Err("root table has the wrong signature");
    }

    for entry in root[SDT_HEADER_LEN..].as_chunks::<8>().0 {
        let address = u64::from_le_bytes(*entry) as usize;
        let header = acpi_bytes(address, SDT_HEADER_LEN)?;
        if &header[..SDT_SIGNATURE_LEN] != signature {
            continue;
        }
        return sdt(address).map(Some);
    }
    Ok(None)
}

/// FADT offsets of the 32-bit DSDT address and its 64-bit ACPI 2.0 successor.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
const FADT_DSDT_OFFSET: usize = 40;
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
const FADT_X_DSDT_OFFSET: usize = 140;

/// The CPU-visible MMIO window ACPI declares for the PCI I/O address space.
///
/// Without port I/O instructions, a root bridge maps PCI I/O space into
/// memory. Firmware describes that mapping only in its ACPI resources: the
/// UEFI root bridge protocol reports the PCI-side range with a zero
/// translation (QEMU virt: 0x0-0xfff), so the window has to come from the
/// DSDT.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IoWindow {
    /// First PCI I/O address the window covers.
    pub pci_min: u64,
    /// Last PCI I/O address the window covers, inclusive.
    pub pci_max: u64,
    /// CPU address at which `pci_min` is mapped; the window is linear.
    pub cpu_base: u64,
}

/// Recovers the PCI I/O window translation from the DSDT's resource bytes.
///
/// This is not AML interpretation: AML resource templates embed ACPI
/// address-space descriptors as fixed-format bytes, the same bytes an OS hands
/// to its PCI host bridge driver. The scan matches DWord/QWord I/O descriptors
/// byte for byte and accepts only a single, arithmetically consistent,
/// translated window; anything ambiguous yields no window.
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn pci_io_window() -> Option<IoWindow> {
    let fadt = find_table(rsdp()?, b"FACP").ok().flatten()?;
    let dsdt_address =
        if fadt.len() >= FADT_X_DSDT_OFFSET + 8 && read_u64(fadt, FADT_X_DSDT_OFFSET) != 0 {
            read_u64(fadt, FADT_X_DSDT_OFFSET) as usize
        } else if fadt.len() >= FADT_DSDT_OFFSET + 4 {
            read_u32(fadt, FADT_DSDT_OFFSET) as usize
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
    // Large resource descriptors start with a tag byte and a 16-bit body
    // length; the DWord and QWord address-space descriptors share one layout
    // that differs only in the width of its address fields.
    /// Tag, body length, and field width of the DWord address-space descriptor.
    const DWORD_IO: (u8, u16, usize) = (0x87, 23, 4);
    /// Tag, body length, and field width of the QWord address-space descriptor.
    const QWORD_IO: (u8, u16, usize) = (0x8a, 43, 8);
    /// The tag and the two length bytes.
    const HEADER_LEN: usize = 3;
    /// Resource type byte after the header; 1 selects an I/O range.
    const TYPE_IO: u8 = 1;
    /// The address fields follow the header, the type, and two flag bytes.
    const FIELDS_OFFSET: usize = 6;
    /// Field order: granularity, minimum, maximum, translation, length.
    const FIELD_MIN: usize = 1;
    const FIELD_MAX: usize = 2;
    const FIELD_TRANSLATION: usize = 3;
    const FIELD_LENGTH: usize = 4;

    let (_, body_len, field_size) = [DWORD_IO, QWORD_IO]
        .into_iter()
        .find(|(tag, _, _)| bytes.first() == Some(tag))?;
    let size = HEADER_LEN + usize::from(body_len);
    if bytes.len() < size
        || u16::from_le_bytes([bytes[1], bytes[2]]) != body_len
        || bytes[HEADER_LEN] != TYPE_IO
    {
        return None;
    }
    let field = |index: usize| {
        let offset = FIELDS_OFFSET + index * field_size;
        if field_size == 8 {
            read_u64(bytes, offset)
        } else {
            u64::from(read_u32(bytes, offset))
        }
    };
    let (pci_min, pci_max, translation, length) = (
        field(FIELD_MIN),
        field(FIELD_MAX),
        field(FIELD_TRANSLATION),
        field(FIELD_LENGTH),
    );

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
    let length = read_u32(header, SDT_LENGTH_OFFSET) as usize;
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

/// Decodes a bounds-checked little-endian 16-bit ACPI field without raw offsets.
fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    let value = bytes[offset..offset + 2]
        .try_into()
        .expect("caller validated ACPI field bounds");
    u16::from_le_bytes(value)
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
