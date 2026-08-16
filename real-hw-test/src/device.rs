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
/// Records how discovery found a candidate so duplicate descriptions remain useful.
pub enum Source {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    RequiredCom1,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    LegacyProbe,
}

#[derive(Debug)]
/// A deduplicated UART address, clock, and all firmware or bus provenance.
pub struct Candidate {
    pub address: Address,
    pub clock_hz: u32,
    pub sources: Vec<Source>,
}

#[derive(Debug, Default)]
/// The candidate list shared by the driver and interactive test phases.
pub struct Inventory {
    candidates: Vec<Candidate>,
}

impl Inventory {
    /// Adds a source to an address, merging descriptions to avoid duplicate tests.
    pub fn add(&mut self, address: Address, clock_hz: Option<u32>, source: Source) {
        if let Some(candidate) = self
            .candidates
            .iter_mut()
            .find(|candidate| candidate.address == address)
        {
            if !candidate.sources.contains(&source) {
                candidate.sources.push(source);
            }
            if let Some(clock_hz) = clock_hz.filter(|clock| *clock != 0) {
                candidate.clock_hz = clock_hz;
            }
            return;
        }

        self.candidates.push(Candidate {
            address,
            clock_hz: clock_hz.unwrap_or(CLK_FREQUENCY_HZ),
            sources: alloc::vec![source],
        });
    }

    /// Returns candidates in discovery order for stable on-screen summaries.
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
}
