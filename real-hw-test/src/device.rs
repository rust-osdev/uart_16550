use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};

use uart_16550::spec::CLK_FREQUENCY_HZ;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A byte-addressable 16550 register block reached through port I/O.
pub enum Address {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    Port(u16),
}

impl Display for Address {
    /// Formats an address in the form used by on-screen diagnostics.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::Port(port) => write!(f, "PIO 0x{port:04x}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// How discovery learned about a candidate; several paths can find one UART.
pub enum Discovery {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    RequiredCom1,
}

impl Display for Discovery {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::RequiredCom1 => "required COM1",
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
/// Where a candidate physically lives; exactly one applies per UART.
pub enum Location {
    /// A conventional I/O port: the Super I/O or LPC UART on the board.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    LegacyPort,
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Self::LegacyPort => f.write_str("built-in legacy port"),
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
