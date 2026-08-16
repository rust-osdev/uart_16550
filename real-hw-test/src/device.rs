use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};

use uart_16550::spec::CLK_FREQUENCY_HZ;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A byte-addressable 16550 register block reached through PIO or MMIO.
pub enum Address {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Port(u16),
    Mmio {
        base: usize,
        stride: u8,
    },
}

impl Display for Address {
    /// Formats an address in the form used by on-screen diagnostics.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Port(port) => write!(f, "PIO 0x{port:04x}"),
            Self::Mmio { base, stride } => {
                write!(f, "MMIO 0x{base:x}, stride {stride}")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// How discovery learned about a candidate; several paths can find one UART.
pub enum Discovery {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    RequiredCom1,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    LegacyProbe,
    AcpiSpcr,
    PciEnumeration,
}

impl Display for Discovery {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::RequiredCom1 => "required COM1",
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::LegacyProbe => "presence check at a conventional port",
            Self::AcpiSpcr => "ACPI SPCR",
            Self::PciEnumeration => "PCI enumeration",
        })
    }
}

/// Formats every discovery path of one candidate as a comma-separated list.
pub struct Discoveries<'a>(&'a [Discovery]);

impl Display for Discoveries<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (index, discovery) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            discovery.fmt(f)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Where a PCI function sits relative to its root bridge.
pub enum Attachment {
    /// Directly on the root bridge's bus: typically a controller integrated on
    /// the board.
    OnRootBus,
    /// Behind a root port or PCI-to-PCI bridge: typically an add-in card.
    BehindBridge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A PCI function's identity from configuration space or the SPCR table.
pub struct PciFunction {
    pub segment: u32,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    /// Unknown while only SPCR has described the function.
    pub attachment: Option<Attachment>,
}

impl PciFunction {
    /// Names devices whose IDs identify them beyond doubt.
    fn known_name(&self) -> Option<&'static str> {
        match (self.vendor_id, self.device_id) {
            (0x1b36, 0x0002) => Some("QEMU pci-serial"),
            (0x1b36, 0x0003) => Some("QEMU pci-serial-2x"),
            (0x1b36, 0x0004) => Some("QEMU pci-serial-4x"),
            _ => None,
        }
    }
}

impl Display for PciFunction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PCI {:04x}:{:02x}:{:02x}.{} {:04x}:{:04x}",
            self.segment, self.bus, self.device, self.function, self.vendor_id, self.device_id
        )?;
        if let Some(name) = self.known_name() {
            write!(f, " ({name})")?;
        }
        match self.attachment {
            Some(Attachment::OnRootBus) => f.write_str(", on the root bus"),
            Some(Attachment::BehindBridge) => f.write_str(", behind a bridge"),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Where a candidate physically lives; exactly one applies per UART.
pub enum Location {
    /// A conventional I/O port: the Super I/O or LPC UART on the board.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    LegacyPort,
    /// A memory-mapped UART that firmware describes without a PCI identity.
    Platform,
    Pci(PciFunction),
}

impl Location {
    /// Prefers PCI evidence: SPCR and enumeration can describe one function,
    /// and only enumeration knows how it is attached.
    fn merge(&mut self, incoming: Location) {
        match (self, incoming) {
            (Self::Pci(current), Self::Pci(incoming)) => {
                if current.attachment.is_none() {
                    current.attachment = incoming.attachment;
                }
            }
            (current, Self::Pci(_)) => *current = incoming,
            _ => {}
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::LegacyPort => f.write_str("built-in legacy port"),
            Self::Platform => f.write_str("built-in platform UART"),
            Self::Pci(function) => function.fmt(f),
        }
    }
}

#[derive(Debug)]
/// A deduplicated UART address, clock, location, and every path that found it.
pub struct Candidate {
    pub address: Address,
    pub clock_hz: u32,
    pub location: Location,
    pub discoveries: Vec<Discovery>,
}

impl Candidate {
    /// Lists the discovery paths for on-screen diagnostics.
    pub fn found_by(&self) -> Discoveries<'_> {
        Discoveries(&self.discoveries)
    }
}

#[derive(Debug, Default)]
/// The candidate list shared by the driver and interactive test phases.
pub struct Inventory {
    candidates: Vec<Candidate>,
}

impl Inventory {
    /// Adds a discovery to an address, merging descriptions so one physical UART
    /// is tested exactly once.
    pub fn add(
        &mut self,
        address: Address,
        clock_hz: Option<u32>,
        discovery: Discovery,
        location: Location,
    ) {
        if let Some(candidate) = self
            .candidates
            .iter_mut()
            .find(|candidate| candidate.address == address)
        {
            if !candidate.discoveries.contains(&discovery) {
                candidate.discoveries.push(discovery);
            }
            candidate.location.merge(location);
            if let Some(clock_hz) = clock_hz.filter(|clock| *clock != 0) {
                candidate.clock_hz = clock_hz;
            }
            return;
        }

        self.candidates.push(Candidate {
            address,
            clock_hz: clock_hz.unwrap_or(CLK_FREQUENCY_HZ),
            location,
            discoveries: alloc::vec![discovery],
        });
    }

    /// Returns candidates in discovery order for stable on-screen summaries.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
}
