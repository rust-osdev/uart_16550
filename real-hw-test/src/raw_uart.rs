//! Minimal synchronous 16550 access for the independent reference path.
//!
//! Direct PIO/MMIO operations validate a candidate before `Uart16550` exists,
//! avoiding a circular test that verifies the driver only with itself.

use core::arch::asm;
use core::hint;

use crate::device::Address;
use crate::uefi;

const DATA: u8 = 0;
const IER: u8 = 1;
const ISR_FCR: u8 = 2;
const LCR: u8 = 3;
const MCR: u8 = 4;
const LSR: u8 = 5;
const MSR: u8 = 6;
const SPR: u8 = 7;

const LCR_DLAB: u8 = 1 << 7;
const LSR_DATA_READY: u8 = 1 << 0;
const LSR_THR_EMPTY: u8 = 1 << 5;
const LSR_TRANSMITTER_EMPTY: u8 = 1 << 6;
const MCR_LOOP_BACK: u8 = 1 << 4;

const POLL_LIMIT: usize = 2_000_000;

/// An independently programmed UART used to establish a hardware baseline.
#[derive(Clone, Copy, Debug)]
pub struct RawUart {
    address: Address,
}

/// A diagnostic snapshot of normal and divisor-latch 16550 registers.
#[derive(Clone, Copy, Debug)]
pub struct RegisterSnapshot {
    pub ier: u8,
    pub isr: u8,
    pub lcr: u8,
    pub mcr: u8,
    pub lsr: u8,
    pub msr: u8,
    pub spr: u8,
    pub dll: u8,
    pub dlm: u8,
}

impl RegisterSnapshot {
    /// Reassembles the divisor-latch bytes captured while DLAB was enabled.
    pub const fn divisor(self) -> u16 {
        (self.dlm as u16) << 8 | self.dll as u16
    }

    /// Prints byte values so hardware-specific deviations remain comparable.
    pub fn print(self, label: &str) {
        uefi::println!(
            concat!(
                "  {}: IER={:02x} ISR={:02x} LCR={:02x} ",
                "MCR={:02x} LSR={:02x} MSR={:02x} SPR={:02x} ",
                "divisor={}",
            ),
            label,
            self.ier,
            self.isr,
            self.lcr,
            self.mcr,
            self.lsr,
            self.msr,
            self.spr,
            self.divisor()
        );
    }
}

/// Identifies the raw preflight stage that detected a non-working UART.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreflightError {
    ScratchRegister,
    InvalidClock,
    TransmitterTimeout,
    ReceiverTimeout,
    UnexpectedByte { expected: u8, actual: u8 },
    UnexpectedMessage,
    RegisterMismatch,
}

impl RawUart {
    /// Creates a reference accessor without touching the candidate yet.
    pub const fn new(address: Address) -> Self {
        Self { address }
    }

    /// Reads one register through the candidate's PIO or MMIO mapping.
    pub fn read(&mut self, offset: u8) -> u8 {
        debug_assert!(offset < 8);
        match self.address {
            Address::Port(base) => {
                let port = base + u16::from(offset);
                let value: u8;
                // SAFETY: discovery assigned an owned 16550-compatible PIO port.
                unsafe {
                    asm!(
                        "in al, dx",
                        in("dx") port,
                        out("al") value,
                        options(nomem, nostack, preserves_flags)
                    );
                }
                value
            }
            Address::Mmio { base, stride } => {
                let address = base + usize::from(offset) * usize::from(stride);
                // SAFETY: discovery validated the live firmware MMIO range.
                unsafe { core::ptr::read_volatile(address as *const u8) }
            }
        }
    }

    /// Writes one register through the candidate's PIO or MMIO mapping.
    pub fn write(&mut self, offset: u8, value: u8) {
        debug_assert!(offset < 8);
        match self.address {
            Address::Port(base) => {
                let port = base + u16::from(offset);
                // SAFETY: discovery assigned an owned 16550-compatible PIO port.
                unsafe {
                    asm!(
                        "out dx, al",
                        in("dx") port,
                        in("al") value,
                        options(nomem, nostack, preserves_flags)
                    );
                }
            }
            Address::Mmio { base, stride } => {
                let address = base + usize::from(offset) * usize::from(stride);
                // SAFETY: discovery validated the live firmware MMIO range.
                unsafe { core::ptr::write_volatile(address as *mut u8, value) };
            }
        }
    }

    /// Writes two patterns and restores the scratch register to detect UARTs.
    pub fn scratch_test(&mut self) -> bool {
        // Restore the old value because firmware may inspect it diagnostically.
        let old = self.read(SPR);
        let passed = [0x42, 0x73].into_iter().all(|pattern| {
            self.write(SPR, pattern);
            self.read(SPR) == pattern
        });
        self.write(SPR, old);
        passed
    }

    /// Captures normal and banked registers while restoring the original LCR.
    pub fn snapshot(&mut self) -> RegisterSnapshot {
        let original_lcr = self.read(LCR);
        self.write(LCR, original_lcr & !LCR_DLAB);
        let ier = self.read(IER);
        let snapshot = RegisterSnapshot {
            ier,
            isr: self.read(ISR_FCR),
            lcr: original_lcr,
            mcr: self.read(MCR),
            lsr: self.read(LSR),
            msr: self.read(MSR),
            spr: self.read(SPR),
            dll: 0,
            dlm: 0,
        };
        self.write(LCR, original_lcr | LCR_DLAB);
        let snapshot = RegisterSnapshot {
            dll: self.read(DATA),
            dlm: self.read(IER),
            ..snapshot
        };
        self.write(LCR, original_lcr);
        snapshot
    }

    /// Programs polling-mode 9600 8N1 after validating the clock and scratch.
    pub fn initialize(&mut self, clock_hz: u32) -> Result<u16, PreflightError> {
        if !self.scratch_test() {
            return Err(PreflightError::ScratchRegister);
        }
        let denominator = 16 * 9_600;
        if clock_hz == 0 || !clock_hz.is_multiple_of(denominator) {
            return Err(PreflightError::InvalidClock);
        }
        let divisor = u16::try_from(clock_hz / denominator)
            .ok()
            .filter(|divisor| *divisor != 0)
            .ok_or(PreflightError::InvalidClock)?;

        self.write(LCR, 0);
        self.write(IER, 0);
        self.write(LCR, LCR_DLAB);
        self.write(DATA, divisor as u8);
        self.write(IER, (divisor >> 8) as u8);
        self.write(LCR, 0x03);
        self.write(ISR_FCR, 0xc7);
        self.write(MCR, 0x0b);
        self.wait_for_lsr(LSR_TRANSMITTER_EMPTY, true)
            .ok_or(PreflightError::TransmitterTimeout)?;

        // Acknowledge stale line/modem deltas inherited from firmware.
        let _ = self.read(LSR);
        let _ = self.read(MSR);
        Ok(divisor)
    }

    /// Checks one-byte and FIFO-sized internal transfers, restoring MCR after.
    pub fn test_loopback(&mut self) -> Result<(), PreflightError> {
        const MESSAGE: [u8; 16] = *b"hello world!1337";
        let old_mcr = self.read(MCR);
        self.write(MCR, MCR_LOOP_BACK);
        self.write(ISR_FCR, 0xc7);
        self.drain_receive_fifo();

        let result = (|| {
            self.send_byte(0x42)?;
            let byte = self.receive_byte()?;
            if byte != 0x42 {
                return Err(PreflightError::UnexpectedByte {
                    expected: 0x42,
                    actual: byte,
                });
            }

            self.wait_for_lsr(LSR_THR_EMPTY, true)
                .ok_or(PreflightError::TransmitterTimeout)?;
            for byte in MESSAGE {
                self.write(DATA, byte);
            }
            let mut received = [0_u8; MESSAGE.len()];
            for byte in &mut received {
                *byte = self.receive_byte()?;
            }
            (received == MESSAGE)
                .then_some(())
                .ok_or(PreflightError::UnexpectedMessage)
        })();

        self.write(MCR, old_mcr);
        self.write(ISR_FCR, 0xc7);
        result
    }

    /// Confirms raw initialization survived loopback and matches invariants.
    pub fn validate_configuration(&mut self, divisor: u16) -> Result<(), PreflightError> {
        self.wait_for_lsr(LSR_THR_EMPTY | LSR_TRANSMITTER_EMPTY, true)
            .ok_or(PreflightError::TransmitterTimeout)?;
        let snapshot = self.snapshot();
        let matches = snapshot.ier == 0
            && snapshot.lcr == 0x03
            && snapshot.mcr & 0x1f == 0x0b
            && snapshot.lsr & (LSR_THR_EMPTY | LSR_TRANSMITTER_EMPTY)
                == LSR_THR_EMPTY | LSR_TRANSMITTER_EMPTY
            && snapshot.isr & 0xc0 == 0xc0
            && snapshot.divisor() == divisor;
        matches
            .then_some(())
            .ok_or(PreflightError::RegisterMismatch)
    }

    /// Sends an entire diagnostic payload with bounded polling per byte.
    pub fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), PreflightError> {
        for &byte in bytes {
            self.send_byte(byte)?;
        }
        Ok(())
    }

    /// Waits for an empty transmit holding register before sending one byte.
    fn send_byte(&mut self, byte: u8) -> Result<(), PreflightError> {
        self.wait_for_lsr(LSR_THR_EMPTY, true)
            .ok_or(PreflightError::TransmitterTimeout)?;
        self.write(DATA, byte);
        Ok(())
    }

    /// Waits for a received byte, bounding failures instead of hanging firmware.
    fn receive_byte(&mut self) -> Result<u8, PreflightError> {
        self.wait_for_lsr(LSR_DATA_READY, true)
            .ok_or(PreflightError::ReceiverTimeout)?;
        Ok(self.read(DATA))
    }

    /// Removes one FIFO of stale input that could otherwise falsify loopback.
    fn drain_receive_fifo(&mut self) {
        for _ in 0..16 {
            if self.read(LSR) & LSR_DATA_READY == 0 {
                break;
            }
            let _ = self.read(DATA);
        }
    }

    /// Polls line-status bits up to a fixed limit and returns the matching LSR.
    fn wait_for_lsr(&mut self, mask: u8, all: bool) -> Option<u8> {
        for _ in 0..POLL_LIMIT {
            let lsr = self.read(LSR);
            let ready = if all {
                lsr & mask == mask
            } else {
                lsr & mask != 0
            };
            if ready {
                return Some(lsr);
            }
            hint::spin_loop();
        }
        None
    }
}
