// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration for [`Uart16550`].
//!
//! [`Uart16550`]: crate::Uart16550

use crate::spec::CLK_FREQUENCY_HZ;
use crate::spec::registers::{FifoTriggerLevel, IER, Parity, WordLength};
use core::cmp::Ordering;

/// The speed of data transmission, measured in symbols (bits) per second.
///
/// This type is a convenient and non-ABI compatible abstraction. Use
/// [`calc_divisor`] to get the divisor for [`DLL`] and [`DLM`].
///
/// [`DLL`]: crate::spec::registers::DLL
/// [`DLM`]: crate::spec::registers::DLM
/// [`calc_divisor`]: crate::spec::calc_divisor
#[allow(missing_docs)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum BaudRate {
    // List of typical baud rates.
    /// The typical baud rate in explicitly configured setups.
    Baud115200,
    Baud57600,
    Baud38400,
    /// The default baud rate in many systems.
    ///
    /// For example, chose this variant  **if your communication partner is a
    /// Linux-based system with default serial configuration**.
    ///
    /// See: <https://docs.kernel.org/admin-guide/serial-console.html>
    #[default]
    Baud9600,
    Baud4800,
    Baud2400,
    Baud1200,
    Baud600,
    Baud300,
    Baud150,
    Baud110,
    Custom(u32),
}

impl BaudRate {
    /// Returns the value as corresponding integer.
    #[must_use]
    pub const fn to_integer(self) -> u32 {
        match self {
            Self::Baud115200 => 115200,
            Self::Baud57600 => 57600,
            Self::Baud38400 => 38400,
            Self::Baud9600 => 9600,
            Self::Baud4800 => 4800,
            Self::Baud2400 => 2400,
            Self::Baud1200 => 1200,
            Self::Baud600 => 600,
            Self::Baud300 => 300,
            Self::Baud150 => 150,
            Self::Baud110 => 110,
            Self::Custom(val) => val,
        }
    }

    /// Try to create the type from an integer representation of the baud rate.
    #[must_use]
    pub const fn from_integer(value: u32) -> Self {
        match value {
            115200 => Self::Baud115200,
            57600 => Self::Baud57600,
            38400 => Self::Baud38400,
            9600 => Self::Baud9600,
            4800 => Self::Baud4800,
            2400 => Self::Baud2400,
            1200 => Self::Baud1200,
            600 => Self::Baud600,
            300 => Self::Baud300,
            150 => Self::Baud150,
            110 => Self::Baud110,
            baud_rate => Self::Custom(baud_rate),
        }
    }
}

impl PartialOrd for BaudRate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BaudRate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.to_integer().cmp(&other.to_integer())
    }
}

/// Configuration for a [`Uart16550`].
///
/// Please note that sender and receiver **must agree** on the transmission
/// settings, otherwise one side will receive garbage.
///
/// # Usage Hints
///
/// Please note that in VMs (e.g., Cloud Hypervisor, QEMU), transmissions
/// settings and baud rate are mostly ignored. To operate on real hardware, you
/// most likely have to fiddle around with the [`BaudRate`] but can stick to
/// `8-N-1` transmission (the default) in most cases.
///
/// # Default Configuration
///
/// See [`Config::DEFAULT`].
///
/// [`Uart16550`]: crate::Uart16550
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Config {
    // Device Config
    /// Interrupts to enable.
    pub interrupts: IER,
    /// The frequency which typically is [`CLK_FREQUENCY_HZ`].
    pub frequency: u32,
    /// The optional prescaler division factor.
    ///
    /// This is a non-standard functionality (i.e., it is not present in the
    /// industry standard 16550 UART). Its purpose is to provide a second
    /// division factor that could be useful in systems which are driven by a
    /// clock multiple of one of the typical frequencies used with this UART.
    pub prescaler_division_factor: Option<u32>,
    /// The [`FifoTriggerLevel`]. If this is `Some`, it will also activate the
    /// internal FIFO with size [`FIFO_SIZE`]. If it is None, the internal
    /// FIFO will be disabled (this can break QEMU, see below).
    ///
    /// # Caution
    ///
    /// Please note that if you set this to `None`, QEMU will never drain the
    /// data and transmitting code might loop endlessly. Probably a broken
    /// device model in QEMU - not using the FIFO however is also uncommon.
    ///
    /// [`FIFO_SIZE`]: crate::spec::FIFO_SIZE
    pub fifo_trigger_level: Option<FifoTriggerLevel>,

    // Transmission Config
    /// The baud rate to use.
    pub baud_rate: BaudRate,
    /// The length of each transmitted word.
    pub data_bits: WordLength,
    /// Whether extra stop bits should be used.
    ///
    /// See [`LCR::MORE_STOP_BITS`] for more info.
    ///
    /// [`LCR::MORE_STOP_BITS`]: crate::spec::registers::LCR::MORE_STOP_BITS
    pub extra_stop_bits: bool,
    /// Whether parity bits should be used.
    pub parity: Parity,
}

impl Config {
    /// The default configuration which works with every Virtual Machine Monitor
    /// (e.g., Cloud Hypervisor, QEMU) and also on real hardware if your
    /// communication partner is a standard Linux with default serial
    /// configuration.
    ///
    /// More precisely, the default configuration uses a [8-N-1] transmission
    /// with a baud rate of [`BaudRate::Baud9600`]. It also activates the FIFO
    /// and the [`IER::DATA_READY`] interrupt.
    ///
    /// [8-N-1]: https://en.wikipedia.org/wiki/Serial_port#Conventional_notation
    pub const DEFAULT: Self = Self {
        // Properties and behavior of the UART
        interrupts: IER::DATA_READY,
        frequency: CLK_FREQUENCY_HZ,
        prescaler_division_factor: None,
        fifo_trigger_level: Some(FifoTriggerLevel::Fourteen),

        // Transmission control
        baud_rate: BaudRate::Baud9600,
        data_bits: WordLength::EightBits,
        extra_stop_bits: false,
        parity: Parity::Disabled,
    };
}

impl Default for Config {
    fn default() -> Self {
        Self::DEFAULT
    }
}
