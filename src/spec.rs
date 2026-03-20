// SPDX-License-Identifier: MIT OR Apache-2.0

//! # Constants, Register Offsets, and Register Bits.
//!
//! Models the raw low-level details as of the [datasheet], and avoids too
//! opinionated abstractions.
//!
//! [datasheet]: https://caro.su/msx/ocm_de1/16550.pdf

pub use crate::spec::errors::*;

/// Most typical 16550 clock frequency of 1.8432 Mhz.
pub const CLK_FREQUENCY_HZ: u32 = 1_843_200;

/// The maximum size of the internal read and write FIFO.
///
/// Each channel (tx: transmission, rx: reception) has its own queue.
pub const FIFO_SIZE: usize = 16;

/// Number of registers of the device.
///
/// The maximum register index is this value minus `1`.
pub const NUM_REGISTERS: usize = 8;

mod errors {
    use core::error::Error;
    use core::fmt::{self, Display, Formatter};

    /// Error that is returned when [`calc_baud_rate`] could not calculate an even
    /// baud rate, i.e., a baud rate that is representable as integer.
    ///
    /// [`calc_baud_rate`]: crate::spec::calc_baud_rate
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Hash)]
    pub struct NonIntegerBaudRateError {
        /// The frequency of the UART 16550.
        pub frequency: u32,
        /// The divisor.
        pub divisor: u32,
        /// The optional prescaler division factor.
        pub prescaler_division_factor: Option<u32>,
    }

    impl Display for NonIntegerBaudRateError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "Input values do not result in even (integer representable) baud rate! frequency={}, divisor={}, prescaler_division_factor={:?}",
                self.frequency, self.divisor, self.prescaler_division_factor
            )
        }
    }

    impl Error for NonIntegerBaudRateError {}

    /// Error that is returned when [`calc_divisor`] could not calculate an even
    /// baud rate, i.e., a baud rate that is representable as integer.
    ///
    /// [`calc_divisor`]: crate::spec::calc_divisor
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Hash)]
    pub struct NonIntegerDivisorError {
        /// The frequency of the UART 16550.
        pub frequency: u32,
        /// The divisor.
        pub baud_rate: u32,
        /// The optional prescaler division factor.
        pub prescaler_division_factor: Option<u32>,
    }

    impl Display for NonIntegerDivisorError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "input values do not result in even (integer representable) baud rate! frequency={}, baud_rate={}, prescaler_division_factor={:?}",
                self.frequency, self.baud_rate, self.prescaler_division_factor,
            )
        }
    }

    impl Error for NonIntegerDivisorError {}
}

/// Calculates the baud rate from the frequency.
///
/// # Arguments
/// - `frequency`: The frequency of the microcontroller, typically
///   [`CLK_FREQUENCY_HZ`].
/// - `divisor`: The divisor to use.
/// - `prescaler_division_factor`: An optional additional division factor in
///   some more modern UART 16550 variants.
pub fn calc_baud_rate(
    frequency: u32,
    divisor: u32,
    prescaler_division_factor: Option<u32>,
) -> Result<u32, NonIntegerBaudRateError> {
    debug_assert_ne!(frequency, 0, "frequency must be non-zero");
    debug_assert_ne!(divisor, 0, "divisor must be non-zero");
    debug_assert!(
        prescaler_division_factor.is_none_or(|psd| psd <= 0xF),
        "prescaler_division_factor must fit in a nibble (0..=15), got {:?}",
        prescaler_division_factor
    );

    let psd = prescaler_division_factor.map_or(0, |psd| psd);
    let a = frequency;
    let b = 16 * (psd + 1) * divisor;

    if a % b == 0 {
        Ok(a / b)
    } else {
        Err(NonIntegerBaudRateError {
            frequency,
            prescaler_division_factor,
            divisor,
        })
    }
}

/// Similar to [`calc_baud_rate`] but with known baud rate to calculate the
/// frequency.
#[must_use]
pub fn calc_frequency(baud_rate: u32, divisor: u32, prescaler_division_factor: Option<u32>) -> u32 {
    debug_assert_ne!(divisor, 0, "divisor must be non-zero");
    debug_assert_ne!(baud_rate, 0, "baud_rate must be non-zero");
    debug_assert!(
        prescaler_division_factor.is_none_or(|psd| psd <= 0xF),
        "prescaler_division_factor must fit in a nibble (0..=15), got {:?}",
        prescaler_division_factor
    );
    let psd = prescaler_division_factor.unwrap_or_default();
    baud_rate * (16 * (psd + 1) * divisor)
}

/// Similar to [`calc_baud_rate`] but with known frequency and baud rate to
/// calculate the divisor.
pub fn calc_divisor(
    frequency: u32,
    baud_rate: u32,
    prescaler_division_factor: Option<u32>,
) -> Result<u16, NonIntegerDivisorError> {
    debug_assert_ne!(frequency, 0, "frequency must be non-zero");
    debug_assert_ne!(baud_rate, 0, "baud_rate must be non-zero");
    debug_assert!(
        prescaler_division_factor.is_none_or(|psd| psd <= 0xF),
        "prescaler_division_factor must fit in a nibble (0..=15), got {:?}",
        prescaler_division_factor
    );

    // This may look counterintuitive but since
    // `divisor = frequency / (16 * baud_rate)`, we can reuse `calc_baud_rate`
    // with different parameters.
    //
    // We have good unit test coverage to check that this is indeed the case.
    calc_baud_rate(frequency, baud_rate, prescaler_division_factor)
        .map_err(|e| NonIntegerDivisorError {
            frequency: e.frequency,
            prescaler_division_factor: e.prescaler_division_factor,
            baud_rate,
        })
        // Unlikely but better be safe with an explicit panic.
        .map(|val| u16::try_from(val).unwrap())
}

/// Exposes low-level information about the on-chip register layout and provides
/// types that model individual registers.
///
/// The getters and setters in this module operate exclusively on raw bit
/// representations within the local computing context. They are limited to
/// extracting or updating the corresponding fields and do not perform direct
/// hardware access.
pub mod registers {
    use bitflags::bitflags;

    /// Provides the register offset from the base register.
    pub mod offsets {

        /// For reads the Receiver Holding Register (RHR) and for writes the
        /// Transmitter Holding Register (THR), effectively acting as
        /// **data** register.
        pub const DATA: usize = 0;

        /// Interrupt Enable Register (IER).
        pub const IER: usize = 1;

        /// Interrupt Status Register (ISR).
        ///
        /// This register is used on **reads** from offset `2`.
        pub const ISR: usize = 2;

        /// FIFO Control Register (FCR).
        ///
        /// This register is used on **writes** to offset `2`.
        pub const FCR: usize = 2;

        /// Line Control Register (LCR).
        pub const LCR: usize = 3;

        /// Modem Control Register (MCR).
        pub const MCR: usize = 4;

        /// Line Status Register (LSR).
        pub const LSR: usize = 5;

        /// Modem Status Register (MSR).
        pub const MSR: usize = 6;

        /// Scratch Pad Register (SPR).
        pub const SPR: usize = 7;

        /* Registers accessible only when DLAB = 1 */

        /// Divisor Latch, Least significant byte (DLL).
        ///
        /// This is the low byte of the 16 bit divisor.
        pub const DLL: usize = 0;

        /// Divisor Latch, Most significant byte (DLM).
        ///
        /// This is the high byte of the 16 bit divisor.
        pub const DLM: usize = 1;

        /// Prescaler Division.
        ///
        /// This is a non-standard register (i.e., it is not present in the
        /// industry standard 16550 UART).
        pub const PSD: usize = 6;
    }

    /// Typing of the data register (RHR / THR).
    pub type DATA = u8;

    bitflags! {
        /// Typing of the Interrupt Enable Register (IER).
        ///
        /// This register individually enables each of the possible interrupt
        /// sources. A logic "1" in any of these bits enables the corresponding
        /// interrupt, while a logic "0" disables it.
        ///
        /// This is a **read/write** register.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct IER: u8 {
            /// Enables the data ready interrupt.
            ///
            /// This means data can be read (again).
            const DATA_READY = 1 << 0;
            /// Enables the THR Empty interrupt.
            ///
            /// This means data can be written (again).
            const THR_EMPTY = 1 << 1;
            /// Enables the Receiver Line Status interrupt.
            ///
            /// This means an error occurred: parity, framing, overrun.
            const RECEIVER_LINE_STATUS = 1 << 2;
            /// Enables the Modem Status interrupt.
            ///
            /// This tells you if the remote is ready for receive.
            const MODEM_STATUS = 1 << 3;
            /// Reserved.
            const _RESERVED0 = 1 << 4;
            /// Reserved.
            const _RESERVED1 = 1 << 5;
            /// Enables the non-standard interrupt issued when a DMA reception
            /// transfer is finished.
            const DMA_RX_END = 1 << 6;
            /// Enables the non-standard interrupt issued when a DMA
            /// transmission transfer is finished.
            const DMA_TX_END = 1 << 7;
        }
    }

    bitflags! {
        /// Typing of the Interrupt Status Register (ISR).
        ///
        /// **Read-only** register at offset [`offsets::ISR`] for identifying
        /// the interrupt with the highest priority that is currently pending.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct ISR: u8 {
            /// Interrupt status bit.
            ///
            /// `0` means an interrupt is pending, `1` means no interrupt is
            /// pending.
            ///
            /// This field reads as "no interrupt is pending", if set.
            const INTERRUPT_STATUS = 1 << 0;
            /// Interrupt Identification Code (IIC, bit 0).
            const IIC_0 = 1 << 1;
            /// Interrupt Identification Code (IIC, bit 1).
            const IIC_1 = 1 << 2;
            /// Interrupt Identification Code (IIC, bit 2).
            const IIC_2 = 1 << 3;
            /// Reflects the state of the dmarx_end input pin which
            /// signals the end of a complete DMA transfer for received data.
            ///
            /// This is a non-standard flag that is enabled only if DMA End
            /// signaling has been enabled with bit 4 of FCR register. Otherwise
            /// it will always be read as '0'.
            const DMA_RX_END = 1 << 4;
            /// Reflects the state of the dmatx_end input pin which
            /// signals the end of a complete DMA transfer for transmitted data.
            /// This is a non-standard flag that is enabled only if DMA End
            /// signaling has been enabled with bit 4 of FCR register. Otherwise
            /// it will always be read as '0'.
            const DMA_TX_END = 1 << 5;
            /// Set if FIFOs are implemented and enabled (by setting FCR bit 0).
            ///
            /// Cleared in non-FIFO (16450) mode.
            const FIFOS_ENABLED0 = 1 << 6;
            /// Set if FIFOs are implemented and enabled (by setting FCR bit 0).
            ///
            /// Cleared in non-FIFO (16450) mode.
            const FIFOS_ENABLED1 = 1 << 7;
        }
    }

    impl ISR {
        /// Returns the matching [`InterruptType`].
        ///
        /// The priority of the interrupt is available via
        /// [`InterruptType::priority`]. If no interrupt is pending, the
        /// function returns `None`.
        #[must_use]
        pub fn interrupt_type(self) -> Option<InterruptType> {
            InterruptType::from_bits(self.bits())
        }

        /// Returns true if there is a pending interrupt.
        #[must_use]
        pub const fn has_pending_interrupt(self) -> bool {
            !self.contains(Self::INTERRUPT_STATUS)
        }
    }

    /// The possible interrupt types reported by the [`ISR`].
    ///
    ///
    /// This type is a convenient and non-ABI compatible abstraction.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum InterruptType {
        /// There is an overrun error, parity error, framing error or break
        /// interrupt indication corresponding to the received data on top of
        /// the receiver's FIFO.
        ///
        /// Note that the FIFO error flag in LSR does not
        /// influence this interrupt, which is related only to the data on top
        /// of the Rx FIFO. This is directly related to the presence of a 1 in
        /// any of the LSR bits 1 to 4.
        ///
        /// **Interrupt reset method:**  Read the Line Status Register (LSR).
        ReceiverLineStatus,
        /// In non-FIFO mode, there is received data available in the RHR
        /// register.
        ///
        /// In FIFO-mode, the number of characters in the reception FIFO is
        /// equal or greater than the trigger level programmed in FCR. Note that
        /// this is not directly related to LSR bit 0, which always indicates
        /// that there is at least one word ready.
        ///
        /// **Interrupt reset method:** Read the Receiver Holding Register (RHR).
        ReceivedDataReady,
        /// There is at least one character in the receiver's FIFO and during a
        /// time corresponding to four characters at the selected baud rate no
        /// new character has been received and no reading has been executed on
        /// the receiver's FIFO.
        ///
        /// **Interrupt reset method:** Read the Receiver Holding Register (RHR).
        ReceptionTimeout,
        /// In non-FIFO mode, the 1-byte THR is empty. In FIFO mode, the
        /// complete 16-byte transmitter's FIFO is empty, so 1 to 16 characters
        /// can be written to THR.
        ///
        /// That is to say, THR Empty bit in LSR is one.
        ///
        /// **Interrupt reset method:** Write the data register. Alternatively,
        /// reading the Interrupt Status Register (ISR) will also clear the
        /// interrupt if this is the interrupt type being currently indicated
        /// (this will not clear the flag in the LSR).
        TransmitterHoldingRegisterEmpty,
        /// A change has been detected in the Clear To Send (CTS), Data Set
        /// Ready (DSR) or Carrier Detect (CD) input lines or a trailing edge
        /// in the Ring Indicator (RI) input line.
        ///
        /// That is to say, at least one of MSR bits 0 to 3 is one.
        ///
        /// **Interrupt reset method:** Read the Modem Status Register (MSR) .
        ModemStatus,
        /// A '1' has been detected in the dmarx_end input pin. This is supposed
        /// to imply the end of a complete DMA transfer for received data,
        /// executed by a DMA controller that provides this signal.
        ///
        /// **Interrupt reset method:** Read the Interrupt Status Register (ISR)
        /// (return of dmarx_end to zero does not reset the interrupt).
        DmaReceptionEndOfTransfer,
        /// A '1' has been detected in the dmatx_end input pin. This is supposed
        /// to imply the end of a complete DMA transfer for received data,
        /// executed by a DMA controller that provides this signal.
        ///
        /// **Interrupt reset method:** Read the Interrupt Status Register (ISR)
        /// (return of dmatx_end to zero does not reset the interrupt).
        DmaTransmissionEndOfTransfer,
    }

    impl InterruptType {
        /// Returns the priority level.
        ///
        /// Priority 1 is highest and 6 is lowest.
        ///
        /// The last two priority levels are not found in standard 16550 UART
        /// and may appear only if the DMA End signaling is enabled
        /// (bit 4 of FCR).
        #[must_use]
        pub const fn priority(self) -> u8 {
            match self {
                Self::ReceiverLineStatus => 1,
                Self::ReceivedDataReady => 2,
                Self::ReceptionTimeout => 2,
                Self::TransmitterHoldingRegisterEmpty => 3,
                Self::ModemStatus => 4,
                Self::DmaReceptionEndOfTransfer => 5,
                Self::DmaTransmissionEndOfTransfer => 6,
            }
        }

        /// Returns the [`InterruptType`] that corresponds to the bits in
        /// [`ISR`].
        ///
        /// If the lowest bit is `0` (interrupt enable), this function
        /// returns `None`
        #[must_use]
        pub fn from_bits(isr_bits: u8) -> Option<Self> {
            let bits = isr_bits & 0xf;

            let has_interrupt = (bits & 1) == 0;
            if !has_interrupt {
                return None;
            }

            let isr_code = (bits >> 1) & 0b111;

            // Taken from the table on page 11/18 in <https://caro.su/msx/ocm_de1/16550.pdf>
            match isr_code {
                0b011 => Some(Self::ReceiverLineStatus),
                0b010 => Some(Self::ReceivedDataReady),
                0b110 => Some(Self::ReceptionTimeout),
                0b001 => Some(Self::TransmitterHoldingRegisterEmpty),
                0b000 => Some(Self::ModemStatus),
                0b111 => Some(Self::DmaReceptionEndOfTransfer),
                0b101 => Some(Self::DmaTransmissionEndOfTransfer),
                0b100 => None,
                // Unreachable bit pattern with the mask we have above
                _ => unreachable!(),
            }
        }
    }

    bitflags! {
        /// Typing of the FIFO Control Register (FCR).
        ///
        /// **Write-only** register at offset [`offsets::FCR`] used to enable or
        /// disable FIFOs, clear receive/transmit FIFOs, and set the receiver
        /// trigger level.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct FCR: u8 {
            /// When set ('1') this bits enables both the transmitter and
            /// receiver FIFOs.
            ///
            /// In any writing to FCR, this bit must be set in order to affect
            /// the rest of the bits, except for bit 4. Changing this bit
            /// automatically resets both FIFOs.
            const FIFO_ENABLE = 1 << 0;
            /// Writing a one to this bit resets the receiver's FIFO (the
            /// pointers are reset and all the words are cleared).
            ///
            /// The Receiver Shift Register is not cleared, so any reception
            /// active will continue. The bit will automatically return to zero.
            const RX_FIFO_RESET = 1 << 1;
            /// Writing a one to this bit resets the transmitter's FIFO (the
            /// pointers are reset).
            ///
            /// The Transmitter Shift Register is not cleared, so any
            /// transmission active will continue. The bit will automatically
            /// return to zero.
            const TX_FIFO_RESET = 1 << 2;
            /// Selects the DMA mode. The DMA mode affects the way in
            /// which the DMA signaling outputs pins (txrdy, rxrdy and their
            /// inverted versions) behave.
            ///
            /// See the DMA signals explanation in the [datasheet] for details.
            ///
            /// [datasheet]: https://caro.su/msx/ocm_de1/16550.pdf
            ///
            /// Mode 0 is intended to transfer one character at a time. Mode 1
            /// is intended to transfer a set of characters at a time.
            ///
            /// # Recommendation
            /// This is typically not set in a kernel.
            const DMA_MODE = 1 << 3;
            /// Enables the DMA End signaling.
            ///
            /// This non-standard feature is useful when the UART is connected
            /// to a DMA controller which provides signals to indicate when a
            /// complete DMA transfer has been completed, either for reception
            /// or transmission (dmaend_rx and dmaend_tx input pins).
            const ENABLE_DMA_END = 1 << 4;
            /// Reserved.
            const _RESERVED0 = 1 << 5;
            /// First bit of [`FifoTriggerLevel`].
            const RX_FIFO_TRIGGER_LEVEL0 = 1 << 6;
            /// Second bit of [`FifoTriggerLevel`].
            const RX_FIFO_TRIGGER_LEVEL1 = 1 << 7;
        }
    }

    impl FCR {
        /// Returns the trigger level of the FIFO.
        #[must_use]
        pub const fn fifo_trigger_level(self) -> FifoTriggerLevel {
            let bits = (self.bits() >> 6) & 0b11;
            FifoTriggerLevel::from_raw_bits(bits)
        }

        /// Sets the trigger level of the FIFO.
        #[must_use]
        pub const fn set_fifo_trigger_level(self, value: FifoTriggerLevel) -> Self {
            let mask = 0b11_u8 << 6;
            let with_relevant_bits_cleared = self.bits() & !mask;
            let base = with_relevant_bits_cleared | (value.to_raw_bits() << 6);
            Self::from_bits_retain(base)
        }
    }

    /// The trigger level for the receiver's FIFO defined in [`FCR`].
    ///
    /// In FIFO mode an interrupt will be generated (if enabled) when the number
    /// of words in the receiver's FIFO is equal or greater than this trigger
    /// level.
    ///
    /// Besides, for FIFO mode operation a time out mechanism is implemented.
    /// Independently of the trigger level of the FIFO, an interrupt will be
    /// generated if there is at least one word in the FIFO and for a time
    /// equivalent to the transmission of four characters.
    ///
    /// This type is a convenient and non-ABI compatible abstraction. ABI
    /// compatibility is given via [`FifoTriggerLevel::from_raw_bits`] and
    /// [`FifoTriggerLevel::to_raw_bits`].
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum FifoTriggerLevel {
        /// Interrupt is created after every character.
        One,
        /// Interrupt is created after every four characters.
        Four,
        /// Interrupt is created after every eight characters.
        Eight,
        /// Interrupt is created after every fourteen characters.
        ///
        /// # Recommendation
        /// This is the recommended default for best system performance.
        #[default]
        Fourteen,
    }

    impl FifoTriggerLevel {
        /// Translates the raw encoding into the corresponding value.
        ///
        /// This function operates on the value as-is and does not perform any
        /// shifting bits.
        #[must_use]
        pub const fn from_raw_bits(bits: u8) -> Self {
            let bits = bits & 0b11;
            match bits {
                0b00 => Self::One,
                0b01 => Self::Four,
                0b10 => Self::Eight,
                0b11 => Self::Fourteen,
                _ => unreachable!(),
            }
        }

        /// Translates the value into the corresponding raw encoding.
        ///
        /// This function operates on the value as-is and does not perform any
        /// shifting bits.
        #[must_use]
        pub const fn to_raw_bits(self) -> u8 {
            match self {
                Self::One => 0b00,
                Self::Four => 0b01,
                Self::Eight => 0b10,
                Self::Fourteen => 0b11,
            }
        }
    }

    bitflags! {
        /// Typing of the Line Control Register (LCR).
        ///
        /// Configures the serial frame format including word length, stop bits,
        /// parity, and controls access to the divisor latches via DLAB.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct LCR: u8 {
            /// First bit of [`WordLength`].
            const WORD_LENGTH0 = 1 << 0;
            /// Second bit of [`WordLength`].
            const WORD_LENGTH1 = 1 << 1;
            /// If cleared, only one stop bit will be transmitted. If set, two
            /// stop bits (1.5 with 5-bit data) will be transmitted before the
            /// start bit of the next character.
            const MORE_STOP_BITS = 1 << 2;
            /// First bit of [`Parity`].
            const PARITY0 = 1 << 3;
            /// Second bit of [`Parity`].
            const PARITY1 = 1 << 4;
            /// Third bit of [`Parity`].
            const PARITY2 = 1 << 5;
            /// When this bit is set a break condition is forced in the
            /// transmission line. The serial output pin (txd) is forced to the
            /// spacing state (zero).
            ///
            /// When this bit is cleared, the break state is removed.
            ///
            /// # Recommendation
            /// Typically, this is not set in console/TTY use-cases.
            const SET_BREAK = 1 << 6;
            /// This is Divisor Latch Access Bit (DLAB).
            ///
            /// This bit **must** be set in order to access the [`DLL`],
            /// [`DLM`], and [`PSD`].
            const DLAB = 1 << 7;
        }
    }

    impl LCR {
        /// Returns the [`WordLength`].
        #[must_use]
        pub const fn word_length(self) -> WordLength {
            let bits = self.bits() & 0b11;
            WordLength::from_raw_bits(bits)
        }

        /// Sets the [`WordLength`].
        #[must_use]
        pub const fn set_word_length(self, value: WordLength) -> Self {
            let mask = 0b11;
            let with_relevant_bits_cleared = self.bits() & !mask;
            let base = with_relevant_bits_cleared | value.to_raw_bits();
            Self::from_bits_retain(base)
        }

        /// Returns the [`Parity`].
        #[must_use]
        pub const fn parity(self) -> Parity {
            let bits = (self.bits() >> 3) & 0b111;
            Parity::from_raw_bits(bits)
        }

        /// Sets the [`Parity`].
        #[must_use]
        pub const fn set_parity(self, value: Parity) -> Self {
            let mask = 0b111_u8 << 3;
            let with_relevant_bits_cleared = self.bits() & !mask;
            let base = with_relevant_bits_cleared | (value.to_raw_bits() << 3);
            Self::from_bits_retain(base)
        }
    }

    /// The length of words for the transmission and reception in [`LCR`].
    ///
    /// This type is a convenient and non-ABI compatible abstraction. ABI
    /// compatibility is given via [`WordLength::from_raw_bits`] and
    /// [`WordLength::to_raw_bits`].
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum WordLength {
        /// 5-bit words.
        FiveBits,
        /// 6-bit words.
        SixBits,
        /// 7-bit words.
        SevenBits,
        /// 8-bit words (recommended default).
        #[default]
        EightBits,
    }

    impl WordLength {
        /// Translates the raw encoding into the corresponding value.
        ///
        /// This function operates on the value as-is and does not perform any
        /// shifting bits.
        #[must_use]
        pub const fn from_raw_bits(bits: u8) -> Self {
            let bits = bits & 0b11;
            match bits {
                0b00 => Self::FiveBits,
                0b01 => Self::SixBits,
                0b10 => Self::SevenBits,
                0b11 => Self::EightBits,
                _ => unreachable!(),
            }
        }

        /// Translates the value into the corresponding raw encoding.
        ///
        /// This function operates on the value as-is and does not perform any
        /// shifting bits.
        #[must_use]
        pub const fn to_raw_bits(self) -> u8 {
            match self {
                Self::FiveBits => 0b00,
                Self::SixBits => 0b01,
                Self::SevenBits => 0b10,
                Self::EightBits => 0b11,
            }
        }

        /// Try to create the type from an integer representation of the word
        /// length.
        ///
        /// Falling back to [`Self::EightBits`] on invalid values.
        #[must_use]
        pub const fn from_integer(value: u8) -> Self {
            match value {
                5 => Self::FiveBits,
                6 => Self::SixBits,
                7 => Self::SevenBits,
                8 => Self::EightBits,
                _ => Self::EightBits,
            }
        }

        /// Returns the value as corresponding integer.
        #[must_use]
        pub const fn to_integer(self) -> u32 {
            match self {
                Self::FiveBits => 5,
                Self::SixBits => 6,
                Self::SevenBits => 7,
                Self::EightBits => 8,
            }
        }
    }

    /// The parity for basic error detection for the transmission as well as
    /// reception.
    ///
    /// This type is a convenient and non-ABI compatible abstraction. ABI
    /// compatibility is given via [`Parity::from_raw_bits`] and
    /// [`Parity::to_raw_bits`].
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Parity {
        /// No parity bit is transmitted nor expected.
        #[default]
        Disabled,
        /// The number of bits including the parity bit must be odd.
        Odd,
        /// The number of bits including the parity bit must be even.
        Even,
        /// The parity bit is sent as/checked to be `1`.
        Forced1,
        /// The parity bit is sent as/checked to be `0`.
        Forced0,
    }

    impl Parity {
        /// Translates the raw encoding into the corresponding value.
        ///
        /// This function operates on the value as-is and does not perform any
        /// shifting bits.
        #[must_use]
        pub const fn from_raw_bits(bits: u8) -> Self {
            let bits = bits & 0b111;
            let disabled = (bits & 1) == 0;
            if disabled {
                return Self::Disabled;
            }
            let bits = bits >> 1;
            match bits {
                0b00 => Self::Odd,
                0b01 => Self::Even,
                0b10 => Self::Forced1,
                0b11 => Self::Forced0,
                // We only have two bits left to check
                _ => unreachable!(),
            }
        }

        /// Translates the value into the corresponding raw encoding.
        ///
        /// This function operates on the value as-is and does not perform any
        /// shifting bits.
        #[must_use]
        pub const fn to_raw_bits(self) -> u8 {
            match self {
                Self::Disabled => 0b000,
                Self::Odd => 0b001,
                Self::Even => 0b011,
                Self::Forced1 => 0b101,
                Self::Forced0 => 0b111,
            }
        }
    }

    bitflags! {
        /// Typing of the Modem Control Register (MCR).
        ///
        /// Controls modem interface output signal.
        ///
        /// This is a **read/write** register.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct MCR: u8 {
            /// Controls the "data terminal ready" active low output (dtr_n).
            ///
            /// Signals the remote that the local UART device is powered on
            /// (hardware is present).
            ///
            /// A 1 in this bit makes dtr_n output a 0. When the bit is cleared,
            /// dtr_n outputs a 1.
            const DTR = 1 << 0;
            /// Controls the "request to send" active low output (rts_n) in the
            /// same way as bit 0 controls dtr_n.
            ///
            /// Signals the remote that the local UART is ready to receive data.
            const RTS = 1 << 1;
            /// Controls the general purpose, active low, output out1_n in the
            /// same way as bit 0 controls dtr_n.
            const OUT_1 = 1 << 2;
            /// Controls the general purpose, active low, output out2_n in
            /// the same way as bit 0 controls dtr_n.
            ///
            /// Besides, in typical x86 systems this acts as a global interrupt
            /// enable bit as it is connected to the systems interrupt
            /// controller. In this case, the complementary interrupt lines
            /// irq and irq_n will become active (1 and 0 respectively) only if
            /// this bit is 1 (and an interrupt condition is taken place).
            const OUT_2_INT_ENABLE = 1 << 3;
            /// Activate the loop back mode. Loop back mode is intended to test
            /// the UART communication.
            ///
            /// The serial output is connected internally to the serial input,
            /// so every character sent is looped back and received.
            const LOOP_BACK = 1 << 4;
            /// Reserved.
            const _RESERVED0 = 1 << 5;
            /// Reserved.
            const _RESERVED1 = 1 << 6;
            /// Reserved.
            const _RESERVED2 = 1 << 7;
        }
    }

    bitflags! {
        /// Typing of the Line Status Register (LSR).
        ///
        /// Reports the current status of the transmitter and receiver,
        ///  including data readiness, errors, and transmitter emptiness.
        ///
        /// This is a **read-only** register.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct LSR: u8 {
            /// It is set if one of more characters have been received and are
            /// waiting in the receiver's FIFO for the user to read them.
            ///
            /// It is zero if there is no available data in the receiver's FIFO.
            const DATA_READY = 1 << 0;
            /// Overrun Error flag. When it is set, a character has been
            /// completely assembled in the Receiver Shift Register without
            /// having free space to put it in the receiver's FIFO or holding
            /// register.
            ///
            /// When an overrun condition appears, the result is different
            /// depending on whether the 16-byte FIFO is active or not.
            const OVERRUN_ERROR = 1 << 1;
            ///  Parity Error flag. When it is set, it indicates that the parity
            /// of the received character is wrong according to the current
            /// setting in LCR.
            ///
            /// This bit is cleared as soon as the LSR is read.
            const PARITY_ERROR = 1 << 2;
            /// Framing Error flag. It indicates that the received character did
            /// not have a valid stop bit (i.e., a 0 was detected in the (first)
            /// stop bit position instead of a 1).
            ///
            /// This bit is cleared as soon as the LSR is read
            const FRAMING_ERROR = 1 << 3;
            /// Break Interrupt indicator. It is set to 1 if the receiver's line
            /// input rxd was held at zero for a complete character time.
            ///
            /// It is to say, the positions corresponding to the start bit, the
            /// data, the parity bit (if any) and the (first) stop bit were all
            /// detected as zeroes. Note that a Frame Error flag always
            /// accompanies this flag.
            ///
            /// This bit is cleared as soon as the LSR is read.
            const BREAK_INTERRUPT = 1 << 4;
            /// Transmit Holding Register Empty flag aka "ready to send".
            /// In non-FIFO mode, this bit is set whenever the 1-byte THR is
            /// empty.
            ///
            /// If the THR holds data to be transmitted, THR is immediately set
            /// when this data is passed to the TSR (Transmitter Shift Register).
            /// **In FIFO mode, this bit is set when the transmitter's FIFO is
            /// completely empty, being 0 if there is at least one byte in the
            /// FIFO waiting to be passed to the TSR for transmission.**
            ///
            /// This bit is cleared when the microprocessor writes new data in
            /// the THR (the data register).
            const THR_EMPTY = 1 << 5;
            /// Transmitter Empty flag. It is 1 when both the THR (or
            /// transmitter's FIFO) and the TSR are empty.
            ///
            /// Reading this bit as 1 means that no transmission is currently
            /// taking place in the txd output pin, the transmission line is
            /// idle.
            ///
            /// As soon as new data is written in the THR, this bit will be
            /// cleared.
            const TRANSMITTER_EMPTY = 1 << 6;
            /// This the FIFO data error bit. If the FIFO is not implemented or
            /// disabled (16450 mode), this bit is always zero.
            ///
            /// If the FIFO is active, this bit will be set as soon as any data
            /// character in the receiver's FIFO has parity or framing error or
            /// the break indication active.
            ///
            /// The bit is cleared when the microprocessor reads the LSR and the
            /// rest of the data in the receiver's FIFO do not have any of these
            /// three associated flags on.
            const FIFO_DATA_ERROR = 1 << 7;
        }
    }

    impl LSR {
        /// Returns `true` if any error flag is set: overrun, parity, framing,
        /// break interrupt, or FIFO data error.
        ///
        /// Useful as a quick pre-check before inspecting individual error bits.
        #[must_use]
        pub const fn has_error(self) -> bool {
            self.intersects(
                Self::OVERRUN_ERROR
                    .union(Self::PARITY_ERROR)
                    .union(Self::FRAMING_ERROR)
                    .union(Self::BREAK_INTERRUPT)
                    .union(Self::FIFO_DATA_ERROR),
            )
        }
    }

    bitflags! {
        /// Typing of the Modem Status Register (MSR).
        ///
        /// Reflects the current state and change status of modem input.
        ///
        /// This is a **read-only** register.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct MSR: u8 {
            /// delta-CTS flag. If set, it means that the cts_n input has
            /// changed since the last time the microprocessor read this
            /// register.
            const DELTA_CTS = 1 << 0;
            /// delta-DSR flag. If set, it means that the dsr_n input has
            /// changed since the last time the microprocessor read this
            /// register.
            const DELTA_DSR = 1 << 1;
            /// Set when a trailing edge is detected in the ri_n input pin, it
            /// is to say, when ri_n changes from 0 to 1.
            const TRAILING_EDGE_RI = 1 << 2;
            /// delta-CD flag. If set, it means that the cd_n input has changed
            /// since the last time the microprocessor read this register.
            const DELTA_CD = 1 << 3;
            /// Clear To Send (CTS) is the complement of the cts_n input.
            ///
            /// This information comes from the remote side and tells if
            /// the remote can receive more data.
            const CTS = 1 << 4;
            /// Data Set Ready (DSR) is the complement of the dsr_n input.
            ///
            /// This information comes from the remote side and tells if
            /// the remote is powered on (hardware is present).
            const DSR = 1 << 5;
            /// Ring Indicator (RI) is the complement of the ri_n input.
            const RI = 1 << 6;
            /// Carrier Detect (CD) is the complement of the cd_n input.
            ///
            /// This information comes from the remote side and tells if
            /// the remote is actively messaged it is there.
            const CD = 1 << 7;
        }
    }

    /// Typing of the Scratch Pad Register (SPR).
    ///
    /// General-purpose read/write register with no defined hardware function,
    /// intended for software use or probing UART presence.
    ///
    /// This is a **read/write** register.
    pub type SPR = u8;

    /// Typing of the divisor latch register (low byte).
    ///
    /// Used to control the effective baud rate (see [`super::calc_baud_rate`]).
    ///
    /// This is a **read/write** register.
    pub type DLL = u8;

    /// Typing of the divisor latch register (high byte).
    ///
    /// Used to control the effective baud rate (see [`super::calc_baud_rate`]).
    ///
    /// This is a **read/write** register.
    pub type DLM = u8;

    /// All legal divisors for [`DLL`] and [`DLM`] that can create a valid and
    /// even baud rate using [`super::calc_baud_rate`].
    #[allow(missing_docs)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    #[repr(u16)]
    pub enum Divisor {
        #[default] // => baud rate 115200
        Divisor1,
        Divisor2,
        Divisor3,
        Divisor4,
        Divisor5,
        Divisor6,
        Divisor8,
        Divisor9,
        Divisor10,
        Divisor12,
        Divisor15,
        Divisor16,
        Divisor18,
        Divisor20,
        Divisor24,
        Divisor25,
        Divisor30,
        Divisor32,
        Divisor36,
        Divisor40,
        Divisor45,
        Divisor48,
        Divisor50,
        Divisor60,
        Divisor64,
        Divisor72,
        Divisor75,
        Divisor80,
        Divisor90,
        Divisor96,
        Divisor100,
        Divisor120,
        Divisor128,
        Divisor144,
        Divisor150,
        Divisor160,
        Divisor180,
        Divisor192,
        Divisor200,
        Divisor225,
        Divisor240,
        Divisor256,
        Divisor288,
        Divisor300,
        Divisor320,
        Divisor360,
        Divisor384,
        Divisor400,
        Divisor450,
        Divisor480,
        Divisor512,
        Divisor576,
        Divisor600,
        Divisor640,
        Divisor720,
        Divisor768,
        Divisor800,
        Divisor900,
        Divisor960,
        Divisor1152,
        Divisor1200,
        Divisor1280,
        Divisor1440,
        Divisor1536,
        Divisor1600,
        Divisor1800,
        Divisor1920,
        Divisor2304,
        Divisor2400,
        Divisor2560,
        Divisor2880,
        Divisor3200,
        Divisor3600,
        Divisor3840,
        Divisor4608,
        Divisor4800,
        Divisor5760,
        Divisor6400,
        Divisor7200,
        Divisor7680,
        Divisor9600,
        Divisor11520,
        Divisor12800,
        Divisor14400,
        Divisor19200,
        Divisor23040,
        Divisor28800,
        Divisor38400,
        Divisor57600,
    }

    bitflags! {
        /// Typing of the Prescaler Division (PSD) register.
        ///
        /// This is a non-standard register (i.e., it is not present in the
        /// industry standard 16550 UART). Its purpose is to provide a second
        /// division factor that could be useful in systems which are driven by
        /// a clock multiple of one of the typical frequencies used with this
        /// UART.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct PSD: u8 {
            /// Prescaler's Division Factor (bit 0).
            const PDF0 = 1 << 0;
            /// Prescaler's Division Factor (bit 1).
            const PDF1 = 1 << 1;
            /// Prescaler's Division Factor (bit 2).
            const PDF2 = 1 << 2;
            /// Prescaler's Division Factor (bit 3).
            const PDF3 = 1 << 3;
            /// Reserved.
            const _RESERVED0 = 1 << 4;
            /// Reserved.
            const _RESERVED1 = 1 << 5;
            /// Reserved.
            const _RESERVED2 = 1 << 6;
            /// Reserved.
            const _RESERVED3 = 1 << 7;
        }
    }

    impl PSD {
        /// Returns the Prescaler's Division Factor (PDF).
        #[must_use]
        pub const fn pdf(self) -> u8 {
            self.bits() & 0xf
        }

        /// Sets the Prescaler's Division Factor (PDF).
        #[must_use]
        pub const fn set_pdf(self, pdf: u8) -> Self {
            debug_assert!(pdf <= 0xf, "pdf must fit in a nibble (0..=15)");
            let mask = 0xf_u8;
            let with_relevant_bits_cleared = self.bits() & !mask;
            let base = with_relevant_bits_cleared | (pdf & 0xf);
            Self::from_bits_retain(base)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::registers::{
        FCR, FifoTriggerLevel, ISR, InterruptType, LCR, PSD, Parity, WordLength,
    };

    // ── Baud rate and divisor ────────────────────────────────────────────────

    #[test]
    fn test_calc_baud_rate() {
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 1, None), Ok(115200));
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 2, None), Ok(57600));
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 3, None), Ok(38400));
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 6, None), Ok(19200));
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 9, None), Ok(12800));
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 12, None), Ok(9600));
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 15, None), Ok(7680));
        assert_eq!(calc_baud_rate(CLK_FREQUENCY_HZ, 16, None), Ok(7200));
        assert_eq!(
            calc_baud_rate(CLK_FREQUENCY_HZ, 73, None),
            Err(NonIntegerBaudRateError {
                frequency: CLK_FREQUENCY_HZ,
                divisor: 73,
                prescaler_division_factor: None,
            })
        );
    }

    #[test]
    fn test_calc_frequency() {
        assert_eq!(calc_frequency(115200, 1, None), CLK_FREQUENCY_HZ);
        assert_eq!(calc_frequency(57600, 2, None), CLK_FREQUENCY_HZ);
        assert_eq!(calc_frequency(38400, 3, None), CLK_FREQUENCY_HZ);
        assert_eq!(calc_frequency(19200, 6, None), CLK_FREQUENCY_HZ);
        assert_eq!(calc_frequency(12800, 9, None), CLK_FREQUENCY_HZ);
        assert_eq!(calc_frequency(9600, 12, None), CLK_FREQUENCY_HZ);
        assert_eq!(calc_frequency(7680, 15, None), CLK_FREQUENCY_HZ);
        assert_eq!(calc_frequency(7200, 16, None), CLK_FREQUENCY_HZ);
    }

    #[test]
    fn test_calc_divisor() {
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 115200, None), Ok(1));
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 57600, None,), Ok(2));
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 38400, None,), Ok(3));
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 19200, None,), Ok(6));
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 12800, None,), Ok(9));
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 9600, None,), Ok(12));
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 7680, None,), Ok(15));
        assert_eq!(calc_divisor(CLK_FREQUENCY_HZ, 7200, None,), Ok(16));
        assert_eq!(
            calc_divisor(CLK_FREQUENCY_HZ, 7211, None),
            Err(NonIntegerDivisorError {
                frequency: CLK_FREQUENCY_HZ,
                baud_rate: 7211,
                prescaler_division_factor: None,
            })
        );
    }

    // ── FCR ──────────────────────────────────────────────────────────────────

    #[test]
    fn fcr_trigger_level_roundtrip_one() {
        let fcr = FCR::empty().set_fifo_trigger_level(FifoTriggerLevel::One);
        assert_eq!(
            fcr.fifo_trigger_level(),
            FifoTriggerLevel::One,
            "trigger level One should survive a set/get round-trip"
        );
        // The raw bit pattern for One is 0b00 << 6 == 0x00; no extra bits set.
        assert_eq!(
            fcr.bits() & 0b0011_1111,
            0,
            "bits [5:0] must be undisturbed"
        );
    }

    #[test]
    fn fcr_trigger_level_roundtrip_four() {
        let fcr = FCR::empty().set_fifo_trigger_level(FifoTriggerLevel::Four);
        assert_eq!(fcr.fifo_trigger_level(), FifoTriggerLevel::Four);
        assert_eq!(fcr.bits(), 0b0100_0000, "Four == 0b01 placed at bits [7:6]");
    }

    #[test]
    fn fcr_trigger_level_roundtrip_eight() {
        let fcr = FCR::empty().set_fifo_trigger_level(FifoTriggerLevel::Eight);
        assert_eq!(fcr.fifo_trigger_level(), FifoTriggerLevel::Eight);
        assert_eq!(
            fcr.bits(),
            0b1000_0000,
            "Eight == 0b10 placed at bits [7:6]"
        );
    }

    #[test]
    fn fcr_trigger_level_roundtrip_fourteen() {
        let fcr = FCR::empty().set_fifo_trigger_level(FifoTriggerLevel::Fourteen);
        assert_eq!(fcr.fifo_trigger_level(), FifoTriggerLevel::Fourteen);
        assert_eq!(
            fcr.bits(),
            0b1100_0000,
            "Fourteen == 0b11 placed at bits [7:6]"
        );
    }

    /// Verify that setting the trigger level does not clobber unrelated FCR bits.
    #[test]
    fn fcr_trigger_level_preserves_other_bits() {
        let base = FCR::FIFO_ENABLE | FCR::DMA_MODE;
        let fcr = base.set_fifo_trigger_level(FifoTriggerLevel::Eight);
        assert!(
            fcr.contains(FCR::FIFO_ENABLE),
            "FIFO_ENABLE must be preserved"
        );
        assert!(fcr.contains(FCR::DMA_MODE), "DMA_MODE must be preserved");
        assert_eq!(fcr.fifo_trigger_level(), FifoTriggerLevel::Eight);
    }

    // ── LCR – word length ────────────────────────────────────────────────────

    /// `set_word_length` is correct (bits [1:0] need no shift), so all four
    /// variants should round-trip cleanly.
    #[test]
    fn lcr_word_length_roundtrip_five() {
        let lcr = LCR::empty().set_word_length(WordLength::FiveBits);
        assert_eq!(lcr.word_length(), WordLength::FiveBits);
        assert_eq!(lcr.bits() & 0b11, 0b00);
    }

    #[test]
    fn lcr_word_length_roundtrip_six() {
        let lcr = LCR::empty().set_word_length(WordLength::SixBits);
        assert_eq!(lcr.word_length(), WordLength::SixBits);
        assert_eq!(lcr.bits() & 0b11, 0b01);
    }

    #[test]
    fn lcr_word_length_roundtrip_seven() {
        let lcr = LCR::empty().set_word_length(WordLength::SevenBits);
        assert_eq!(lcr.word_length(), WordLength::SevenBits);
        assert_eq!(lcr.bits() & 0b11, 0b10);
    }

    #[test]
    fn lcr_word_length_roundtrip_eight() {
        let lcr = LCR::empty().set_word_length(WordLength::EightBits);
        assert_eq!(lcr.word_length(), WordLength::EightBits);
        assert_eq!(lcr.bits() & 0b11, 0b11);
    }

    /// `set_word_length` must not disturb bits [7:2].
    #[test]
    fn lcr_word_length_preserves_other_bits() {
        let base = LCR::DLAB | LCR::MORE_STOP_BITS;
        let lcr = base.set_word_length(WordLength::SevenBits);
        assert!(lcr.contains(LCR::DLAB), "DLAB must be preserved");
        assert!(
            lcr.contains(LCR::MORE_STOP_BITS),
            "MORE_STOP_BITS must be preserved"
        );
        assert_eq!(lcr.word_length(), WordLength::SevenBits);
    }

    /// set_word_length is in bits [1:0] so the OR-only bug only manifests
    /// when the new value has a 0 in a bit position the old value had a 1.
    /// EightBits (0b11) -> FiveBits (0b00) is the worst case.
    #[test]
    fn lcr_set_word_length_twice_no_stale_bits() {
        let lcr = LCR::empty()
            .set_word_length(WordLength::EightBits) // 0b11
            .set_word_length(WordLength::FiveBits); // 0b00; without clear -> 0b11 (EightBits)
        assert_eq!(
            lcr.word_length(),
            WordLength::FiveBits,
            "stale EightBits bits must be cleared before setting FiveBits"
        );
    }

    // ── LCR – parity ─────────────────────────────────────────────────────────

    #[test]
    fn lcr_parity_roundtrip_disabled() {
        let lcr = LCR::empty().set_parity(Parity::Disabled);
        assert_eq!(
            lcr.parity(),
            Parity::Disabled,
            "Disabled parity should round-trip"
        );
        // Disabled == 0b000 << 3; no parity bits set at all.
        assert_eq!(lcr.bits() & 0b0011_1000, 0);
    }

    #[test]
    fn lcr_parity_roundtrip_odd() {
        let lcr = LCR::empty().set_parity(Parity::Odd);
        assert_eq!(lcr.parity(), Parity::Odd);
        // Odd == 0b001 << 3 == 0b000_1000
        assert_eq!(lcr.bits() & 0b0011_1000, 0b000_1000);
    }

    #[test]
    fn lcr_parity_roundtrip_even() {
        let lcr = LCR::empty().set_parity(Parity::Even);
        assert_eq!(lcr.parity(), Parity::Even);
        // Even == 0b011 << 3 == 0b001_1000
        assert_eq!(lcr.bits() & 0b0011_1000, 0b001_1000);
    }

    #[test]
    fn lcr_parity_roundtrip_forced1() {
        let lcr = LCR::empty().set_parity(Parity::Forced1);
        assert_eq!(lcr.parity(), Parity::Forced1);
        // Forced1 == 0b101 << 3 == 0b010_1000
        assert_eq!(lcr.bits() & 0b0011_1000, 0b010_1000);
    }

    #[test]
    fn lcr_parity_roundtrip_forced0() {
        let lcr = LCR::empty().set_parity(Parity::Forced0);
        assert_eq!(lcr.parity(), Parity::Forced0);
        // Forced0 == 0b111 << 3 == 0b011_1000
        assert_eq!(lcr.bits() & 0b0011_1000, 0b011_1000);
    }

    /// Parity setter must not clobber word-length or control bits.
    #[test]
    fn lcr_parity_preserves_other_bits() {
        let base = LCR::DLAB | LCR::empty().set_word_length(WordLength::EightBits);
        let lcr = base.set_parity(Parity::Even);
        assert!(
            lcr.contains(LCR::DLAB),
            "DLAB must survive a set_parity call"
        );
        assert_eq!(
            lcr.word_length(),
            WordLength::EightBits,
            "word length must survive"
        );
        assert_eq!(lcr.parity(), Parity::Even);
    }

    /// Calling set_parity twice must reflect only the *last* value.
    /// This is the exact bug that was fixed — OR-only without clearing first
    /// would leave the old parity bits set.
    #[test]
    fn lcr_set_parity_twice_no_stale_bits() {
        let lcr = LCR::empty()
            .set_parity(Parity::Odd)
            .set_parity(Parity::Even);
        assert_eq!(lcr.parity(), Parity::Even, "second set_parity must win");
        // Odd is 0b001<<3, Even is 0b011<<3. Without clearing, OR would
        // produce 0b011 which happens to equal Even here — use Forced0 vs Odd
        // to get a case where the stale bit actually corrupts the result.
        let lcr = LCR::empty()
            .set_parity(Parity::Forced0) // 0b111 << 3
            .set_parity(Parity::Odd); // 0b001 << 3; without clear -> 0b111 (Forced0 again)
        assert_eq!(
            lcr.parity(),
            Parity::Odd,
            "stale Forced0 bits must be cleared before setting Odd"
        );
    }

    // ── PSD ──────────────────────────────────────────────────────────────────

    /// Basic round-trips for `set_pdf` / `pdf`.
    #[test]
    fn psd_pdf_roundtrip_zero() {
        let psd = PSD::empty().set_pdf(0);
        assert_eq!(psd.pdf(), 0);
    }

    #[test]
    fn psd_pdf_roundtrip_max_nibble() {
        let psd = PSD::empty().set_pdf(0xF);
        assert_eq!(psd.pdf(), 0xF);
    }

    /// Verify that set_pdf preserves reserved bits in the upper nibble.
    #[test]
    fn psd_set_pdf_preserves_reserved_bits() {
        // Manually construct a PSD value with a reserved bit set (bit 4).
        let base = PSD::from_bits_retain(0b0001_0000); // _RESERVED0 set
        let psd = base.set_pdf(0x5);
        assert_eq!(psd.pdf(), 0x5, "pdf value must be stored correctly");
        assert_eq!(
            psd.bits() & 0b1111_0000,
            0b0001_0000,
            "reserved bits must be preserved by set_pdf"
        );
    }

    /// Same for PSD: set_pdf called twice must reflect only the last nibble.
    #[test]
    fn psd_set_pdf_twice_no_stale_bits() {
        let psd = PSD::empty().set_pdf(0xF).set_pdf(0x0);
        assert_eq!(
            psd.pdf(),
            0x0,
            "stale high nibble bits must be cleared before setting 0"
        );
    }

    // ── InterruptType::from_bits ─────────────────────────────────────────────

    /// No interrupt is signalled when bit 0 is set (active-low sense).
    #[test]
    fn interrupt_type_no_interrupt_when_bit0_set() {
        // Any value with bit 0 == 1 means "no interrupt pending".
        assert!(InterruptType::from_bits(0b0000_0001).is_none());
        assert!(InterruptType::from_bits(0xFF).is_none());
        assert!(InterruptType::from_bits(0b0000_1111).is_none());
    }

    /// All 7 interrupt types decode correctly from their ISR bit patterns.
    #[test]
    fn interrupt_type_all_variants() {
        // Bit 0 must be 0 (interrupt pending); bits [3:1] are the IIC.
        // Pattern: IIC shifted left by 1 (bit 0 = 0).
        let cases = [
            (0b0110, InterruptType::ReceiverLineStatus),
            (0b0100, InterruptType::ReceivedDataReady),
            (0b1100, InterruptType::ReceptionTimeout),
            (0b0010, InterruptType::TransmitterHoldingRegisterEmpty),
            (0b0000, InterruptType::ModemStatus),
            (0b1110, InterruptType::DmaReceptionEndOfTransfer),
            (0b1010, InterruptType::DmaTransmissionEndOfTransfer),
        ];
        for (bits, expected) in cases {
            assert_eq!(
                InterruptType::from_bits(bits),
                Some(expected),
                "bits {bits:#010b} should decode to {expected:?}"
            );
        }
    }

    /// Bits [7:4] are ignored — only the low nibble matters.
    #[test]
    fn interrupt_type_ignores_high_nibble() {
        // ReceivedDataReady == 0b0100; high nibble should be masked away.
        assert_eq!(
            InterruptType::from_bits(0b1111_0100),
            InterruptType::from_bits(0b0000_0100),
        );
    }

    /// Priority ordering: lower number == higher priority.
    #[test]
    fn interrupt_type_priority_ordering() {
        assert!(
            InterruptType::ReceiverLineStatus.priority()
                < InterruptType::ReceivedDataReady.priority()
        );
        assert_eq!(
            InterruptType::ReceivedDataReady.priority(),
            InterruptType::ReceptionTimeout.priority(),
            "ReceivedDataReady and ReceptionTimeout share priority 2"
        );
        assert!(
            InterruptType::TransmitterHoldingRegisterEmpty.priority()
                < InterruptType::ModemStatus.priority()
        );
        assert!(
            InterruptType::DmaReceptionEndOfTransfer.priority()
                < InterruptType::DmaTransmissionEndOfTransfer.priority()
        );
    }

    /// ISR::interrupt_type is a thin wrapper — verify it agrees with from_bits.
    #[test]
    fn isr_interrupt_type_delegates_correctly() {
        let isr = ISR::from_bits_retain(0b0100); // ReceivedDataReady
        assert_eq!(isr.interrupt_type(), Some(InterruptType::ReceivedDataReady));

        let isr = ISR::from_bits_retain(0b0001); // no interrupt
        assert_eq!(isr.interrupt_type(), None);
    }

    // ── Parity::from_raw_bits ────────────────────────────────────────────────

    #[test]
    fn parity_from_raw_bits_all_variants() {
        assert_eq!(Parity::from_raw_bits(0b000), Parity::Disabled);
        assert_eq!(Parity::from_raw_bits(0b001), Parity::Odd);
        assert_eq!(Parity::from_raw_bits(0b011), Parity::Even);
        assert_eq!(Parity::from_raw_bits(0b101), Parity::Forced1);
        assert_eq!(Parity::from_raw_bits(0b111), Parity::Forced0);
    }

    /// Any even raw value (bit 0 == 0) must yield Disabled.
    #[test]
    fn parity_from_raw_bits_any_even_is_disabled() {
        assert_eq!(Parity::from_raw_bits(0b000), Parity::Disabled);
        assert_eq!(Parity::from_raw_bits(0b010), Parity::Disabled);
        assert_eq!(Parity::from_raw_bits(0b100), Parity::Disabled);
        assert_eq!(Parity::from_raw_bits(0b110), Parity::Disabled);
    }

    /// The `& 0b111` mask must drop any bits above bit 2.
    #[test]
    fn parity_from_raw_bits_masks_high_bits() {
        assert_eq!(Parity::from_raw_bits(0b1111_1001), Parity::Odd);
        assert_eq!(Parity::from_raw_bits(0b1111_1011), Parity::Even);
    }

    /// to_raw_bits / from_raw_bits must form a perfect round-trip.
    #[test]
    fn parity_raw_bits_roundtrip() {
        for p in [
            Parity::Disabled,
            Parity::Odd,
            Parity::Even,
            Parity::Forced1,
            Parity::Forced0,
        ] {
            assert_eq!(Parity::from_raw_bits(p.to_raw_bits()), p);
        }
    }
    // ── FifoTriggerLevel raw bits ────────────────────────────────────────────

    #[test]
    fn fifo_trigger_level_raw_bits_roundtrip() {
        for level in [
            FifoTriggerLevel::One,
            FifoTriggerLevel::Four,
            FifoTriggerLevel::Eight,
            FifoTriggerLevel::Fourteen,
        ] {
            assert_eq!(FifoTriggerLevel::from_raw_bits(level.to_raw_bits()), level);
        }
    }

    /// High bits must be masked away in from_raw_bits.
    #[test]
    fn fifo_trigger_level_from_raw_bits_masks_high_bits() {
        assert_eq!(
            FifoTriggerLevel::from_raw_bits(0b1111_1100),
            FifoTriggerLevel::One
        );
        assert_eq!(
            FifoTriggerLevel::from_raw_bits(0b1111_1101),
            FifoTriggerLevel::Four
        );
    }

    /// Same regression test for set_fifo_trigger_level.
    #[test]
    fn fcr_set_trigger_level_twice_no_stale_bits() {
        let fcr = FCR::empty()
            .set_fifo_trigger_level(FifoTriggerLevel::Fourteen) // 0b11 << 6
            .set_fifo_trigger_level(FifoTriggerLevel::Four); // 0b01 << 6; without clear -> 0b11 (Fourteen)
        assert_eq!(
            fcr.fifo_trigger_level(),
            FifoTriggerLevel::Four,
            "stale Fourteen bits must be cleared before setting Four"
        );
    }

    // ── WordLength raw bits ──────────────────────────────────────────────────

    #[test]
    fn word_length_raw_bits_roundtrip() {
        for wl in [
            WordLength::FiveBits,
            WordLength::SixBits,
            WordLength::SevenBits,
            WordLength::EightBits,
        ] {
            assert_eq!(WordLength::from_raw_bits(wl.to_raw_bits()), wl);
        }
    }

    /// from_integer / to_integer must round-trip for the four legal values.
    #[test]
    fn word_length_integer_roundtrip() {
        for n in [5u8, 6, 7, 8] {
            let wl = WordLength::from_integer(n);
            assert_eq!(wl.to_integer() as u8, n);
        }
    }

    /// Out-of-range values silently fall back to EightBits — pin that contract.
    #[test]
    fn word_length_from_integer_out_of_range_fallback() {
        assert_eq!(WordLength::from_integer(0), WordLength::EightBits);
        assert_eq!(WordLength::from_integer(4), WordLength::EightBits);
        assert_eq!(WordLength::from_integer(9), WordLength::EightBits);
        assert_eq!(WordLength::from_integer(255), WordLength::EightBits);
    }

    // ── LSR ──────────────────────────────────────────────────────────────────

    use crate::spec::registers::LSR;

    #[test]
    fn lsr_has_error_false_when_clean() {
        assert!(!LSR::empty().has_error());
        assert!(!LSR::DATA_READY.has_error());
        assert!(!LSR::THR_EMPTY.has_error());
        assert!(!LSR::TRANSMITTER_EMPTY.has_error());
    }

    #[test]
    fn lsr_has_error_true_for_each_error_flag() {
        assert!(LSR::OVERRUN_ERROR.has_error());
        assert!(LSR::PARITY_ERROR.has_error());
        assert!(LSR::FRAMING_ERROR.has_error());
        assert!(LSR::BREAK_INTERRUPT.has_error());
        assert!(LSR::FIFO_DATA_ERROR.has_error());
    }
}
