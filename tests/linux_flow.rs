//! Tests the driver against a model of a 16550A, following the flows of the
//! Linux 8250 driver (`drivers/tty/serial/8250/8250_port.c`).
//!
//! The model is hooked into the driver as custom [`Backend`]. It is
//! deliberately independent of the driver's register definitions and follows
//! the 16550A datasheet and what Linux relies on instead.

use std::collections::VecDeque;
use uart_16550::backend::Backend;
use uart_16550::spec::CLK_FREQUENCY_HZ;
use uart_16550::spec::registers::{FifoTriggerLevel, IER, LSR, MSR, Parity, WordLength};
use uart_16550::{BaudRate, Config, Uart16550};

/// A model of a 16550A UART.
///
/// Time advances with every register access. Transmitting a character takes
/// [`Device16550::CHAR_TICKS`] ticks.
// The model is complete; not every test uses all of it.
#[allow(dead_code)]
mod model {
    use super::*;

    // Register offsets, see `include/uapi/linux/serial_reg.h`.
    pub const RX: u8 = 0;
    pub const TX: u8 = 0;
    pub const DLL: u8 = 0;
    pub const IER: u8 = 1;
    pub const DLM: u8 = 1;
    pub const IIR: u8 = 2;
    pub const FCR: u8 = 2;
    pub const LCR: u8 = 3;
    pub const MCR: u8 = 4;
    pub const LSR: u8 = 5;
    pub const MSR: u8 = 6;
    pub const SCR: u8 = 7;

    pub const IER_RDI: u8 = 0x01;
    pub const IER_THRI: u8 = 0x02;
    pub const IER_RLSI: u8 = 0x04;
    pub const IER_MSI: u8 = 0x08;

    pub const IIR_NO_INT: u8 = 0x01;
    pub const IIR_MSI: u8 = 0x00;
    pub const IIR_THRI: u8 = 0x02;
    pub const IIR_RDI: u8 = 0x04;
    pub const IIR_RLSI: u8 = 0x06;
    pub const IIR_RX_TIMEOUT: u8 = 0x0c;
    pub const IIR_FIFO_ENABLED: u8 = 0xc0;

    pub const FCR_ENABLE_FIFO: u8 = 0x01;
    pub const FCR_CLEAR_RCVR: u8 = 0x02;
    pub const FCR_CLEAR_XMIT: u8 = 0x04;

    pub const LCR_DLAB: u8 = 0x80;
    pub const LCR_WLEN8: u8 = 0x03;

    pub const MCR_DTR: u8 = 0x01;
    pub const MCR_RTS: u8 = 0x02;
    pub const MCR_OUT1: u8 = 0x04;
    pub const MCR_OUT2: u8 = 0x08;
    pub const MCR_LOOP: u8 = 0x10;

    pub const LSR_DR: u8 = 0x01;
    pub const LSR_OE: u8 = 0x02;
    pub const LSR_PE: u8 = 0x04;
    pub const LSR_FE: u8 = 0x08;
    pub const LSR_BI: u8 = 0x10;
    pub const LSR_THRE: u8 = 0x20;
    pub const LSR_TEMT: u8 = 0x40;
    pub const LSR_FIFOE: u8 = 0x80;
    pub const LSR_BRK_ERROR_BITS: u8 = LSR_BI | LSR_FE | LSR_PE | LSR_OE;

    pub const MSR_DCTS: u8 = 0x01;
    pub const MSR_DDSR: u8 = 0x02;
    pub const MSR_TERI: u8 = 0x04;
    pub const MSR_DDCD: u8 = 0x08;
    pub const MSR_CTS: u8 = 0x10;
    pub const MSR_DSR: u8 = 0x20;
    pub const MSR_RI: u8 = 0x40;
    pub const MSR_DCD: u8 = 0x80;

    pub const FIFO_SIZE: usize = 16;

    /// A received character together with its error flags (PE, FE, BI).
    #[derive(Clone, Copy, Debug)]
    struct RxChar {
        data: u8,
        errors: u8,
    }

    /// A model of a 16550A UART.
    #[derive(Debug)]
    pub struct Device16550 {
        /// Interrupt Enable Register.
        ier: u8,
        /// Line Control Register.
        lcr: u8,
        /// Modem Control Register.
        mcr: u8,
        /// Scratch Register.
        scr: u8,
        /// Divisor Latch, low byte.
        dll: u8,
        /// Divisor Latch, high byte.
        dlm: u8,
        /// Whether the FIFOs are enabled via FCR.
        fifo_enabled: bool,
        /// The RX FIFO level that raises the received data interrupt.
        rx_trigger: usize,
        /// The RX FIFO, or RBR without FIFO.
        rx: VecDeque<RxChar>,
        /// The TX FIFO, or THR without FIFO.
        tx: VecDeque<u8>,
        /// The character in the transmitter shift register and the remaining
        /// ticks until it is sent.
        tsr: Option<(u8, usize)>,
        /// Whether a received character was lost, reported as LSR OE.
        overrun: bool,
        /// The last byte read from RBR, which an empty RBR returns again.
        last_rx: u8,
        /// Whether the errors of the character on top of the RX FIFO were
        /// already reported by an LSR read.
        top_errors_reported: bool,
        /// The THRE interrupt condition, cleared by IIR reads and THR writes.
        thre_pending: bool,
        /// Ticks since the last RX FIFO activity.
        rx_idle_ticks: usize,
        /// Modem input lines driven by the remote (CTS, DSR, RI, DCD).
        modem_inputs: u8,
        /// The modem input lines as seen in the MSR, including loopback.
        msr_lines: u8,
        /// The MSR change indicators (DCTS, DDSR, TERI, DDCD).
        msr_delta: u8,
        /// Bytes that went out on the wire.
        sent: Vec<u8>,
        /// Bytes written to THR while the TX FIFO was full.
        pub tx_overflows: usize,
        /// Whether the chip has FIFOs. Without, it behaves like a 16450.
        has_fifo: bool,
        /// XORed into looped-back data, to simulate a broken loopback path.
        pub loopback_xor: u8,
        /// Register accesses, to catch drivers that spin forever.
        pub accesses: usize,
        /// Logged register accesses: (offset, is_write, value).
        pub log: Vec<(u8, bool, u8)>,
    }

    impl Device16550 {
        /// Ticks to transmit one character.
        pub const CHAR_TICKS: usize = 4;
        /// Ticks without RX FIFO activity until the character timeout
        /// interrupt, i.e., four character times.
        pub const RX_TIMEOUT_TICKS: usize = 4 * Self::CHAR_TICKS;
        /// Register accesses after which we assume the driver spins forever.
        pub const ACCESS_BUDGET: usize = 100_000;

        /// Creates the device in its reset state.
        pub fn new() -> Self {
            Self {
                ier: 0,
                lcr: 0,
                mcr: 0,
                scr: 0,
                dll: 0,
                dlm: 0,
                fifo_enabled: false,
                rx_trigger: 1,
                rx: VecDeque::new(),
                tx: VecDeque::new(),
                tsr: None,
                overrun: false,
                last_rx: 0,
                top_errors_reported: false,
                thre_pending: false,
                rx_idle_ticks: 0,
                modem_inputs: 0,
                msr_lines: 0,
                msr_delta: 0,
                sent: Vec::new(),
                tx_overflows: 0,
                has_fifo: true,
                loopback_xor: 0,
                accesses: 0,
                log: Vec::new(),
            }
        }

        /// Creates a 16450, which has no FIFOs, in its reset state.
        pub fn new_16450() -> Self {
            Self {
                has_fifo: false,
                ..Self::new()
            }
        }

        /* ----- Remote side ------------------------------------------------ */

        /// The remote sends bytes to us. In loopback mode, they don't arrive.
        pub fn inject_rx(&mut self, bytes: &[u8]) {
            if self.mcr & MCR_LOOP != 0 {
                return;
            }
            for &data in bytes {
                self.push_rx(RxChar { data, errors: 0 });
            }
        }

        /// The remote sends a byte that is received with errors (PE, FE, BI).
        pub fn inject_rx_with_errors(&mut self, data: u8, errors: u8) {
            assert_eq!(errors & !(LSR_PE | LSR_FE | LSR_BI), 0);
            if self.mcr & MCR_LOOP != 0 {
                return;
            }
            self.push_rx(RxChar { data, errors });
        }

        /// Returns and clears the bytes that went out on the wire.
        pub fn take_sent(&mut self) -> Vec<u8> {
            core::mem::take(&mut self.sent)
        }

        /// Sets the modem input lines driven by the remote.
        pub fn set_modem_inputs(&mut self, lines: u8) {
            assert_eq!(lines & !(MSR_CTS | MSR_DSR | MSR_RI | MSR_DCD), 0);
            self.modem_inputs = lines;
            self.update_msr_lines();
        }

        /// Whether the interrupt line is asserted.
        ///
        /// On PCs, OUT2 gates the IRQ line, and it is not driven in loopback
        /// mode.
        pub fn irq(&self) -> bool {
            self.mcr & MCR_OUT2 != 0 && self.mcr & MCR_LOOP == 0 && self.iir_id() != IIR_NO_INT
        }

        /// Lets time pass without register accesses.
        pub fn idle(&mut self, ticks: usize) {
            for _ in 0..ticks {
                self.tick();
            }
        }

        /// The number of bytes in the RX FIFO.
        pub fn rx_level(&self) -> usize {
            self.rx.len()
        }

        /// The divisor latch value.
        pub fn divisor(&self) -> u16 {
            u16::from_le_bytes([self.dll, self.dlm])
        }

        /// Whether the FIFOs are enabled.
        pub fn fifo_enabled(&self) -> bool {
            self.fifo_enabled
        }

        /// Peeks at a register without side effects.
        pub fn peek(&self, offset: u8) -> u8 {
            match offset {
                IER => self.ier,
                LCR => self.lcr,
                MCR => self.mcr,
                SCR => self.scr,
                IIR => self.iir_id() | self.iir_fifo_bits(),
                LSR => self.lsr(),
                MSR => self.msr_lines | self.msr_delta,
                _ => panic!("register {offset} can't be peeked at"),
            }
        }

        /* ----- Internals -------------------------------------------------- */

        /// The number of characters each FIFO holds, 1 without FIFO.
        fn fifo_capacity(&self) -> usize {
            if self.fifo_enabled { FIFO_SIZE } else { 1 }
        }

        /// Receives a character into the RX FIFO, or flags an overrun.
        fn push_rx(&mut self, c: RxChar) {
            if self.rx.len() == self.fifo_capacity() {
                self.overrun = true;
                if !self.fifo_enabled {
                    // Without FIFO, the new character overwrites RBR. With
                    // FIFO, it is lost.
                    self.rx[0] = c;
                    self.top_errors_reported = false;
                }
            } else {
                if self.rx.is_empty() {
                    self.top_errors_reported = false;
                }
                self.rx.push_back(c);
            }
            self.rx_idle_ticks = 0;
        }

        /// Advances time by one tick: moves TX data through the shift register.
        fn tick(&mut self) {
            if let Some((data, ticks)) = self.tsr.as_mut() {
                *ticks -= 1;
                if *ticks == 0 {
                    let data = *data;
                    self.tsr = None;
                    if self.mcr & MCR_LOOP != 0 {
                        let data = data ^ self.loopback_xor;
                        self.push_rx(RxChar { data, errors: 0 });
                    } else {
                        self.sent.push(data);
                    }
                }
            }
            if self.tsr.is_none()
                && let Some(data) = self.tx.pop_front()
            {
                self.tsr = Some((data, Self::CHAR_TICKS));
                if self.tx.is_empty() {
                    self.thre_pending = true;
                }
            }
            self.rx_idle_ticks = self.rx_idle_ticks.saturating_add(1);
        }

        /// The LSR value, without the side effects of reading it.
        fn lsr(&self) -> u8 {
            let mut lsr = 0;
            if let Some(top) = self.rx.front() {
                lsr |= LSR_DR;
                if !self.top_errors_reported {
                    lsr |= top.errors;
                }
            }
            if self.overrun {
                lsr |= LSR_OE;
            }
            if self.tx.is_empty() {
                lsr |= LSR_THRE;
                if self.tsr.is_none() {
                    lsr |= LSR_TEMT;
                }
            }
            // An error anywhere in the RX FIFO that wasn't reported yet.
            let unreported_error = self
                .rx
                .iter()
                .enumerate()
                .any(|(i, c)| c.errors != 0 && !(i == 0 && self.top_errors_reported));
            if self.fifo_enabled && unreported_error {
                lsr |= LSR_FIFOE;
            }
            lsr
        }

        /// Whether the receiver line status interrupt condition is met.
        fn line_status_pending(&self) -> bool {
            self.lsr() & LSR_BRK_ERROR_BITS != 0
        }

        /// Whether the received data interrupt condition is met.
        fn rx_data_pending(&self) -> bool {
            if self.fifo_enabled {
                self.rx.len() >= self.rx_trigger
            } else {
                !self.rx.is_empty()
            }
        }

        /// Whether the character timeout interrupt condition is met.
        fn rx_timeout_pending(&self) -> bool {
            self.fifo_enabled && !self.rx.is_empty() && self.rx_idle_ticks >= Self::RX_TIMEOUT_TICKS
        }

        /// IIR bits 7:6, which report enabled FIFOs.
        fn iir_fifo_bits(&self) -> u8 {
            if self.fifo_enabled {
                IIR_FIFO_ENABLED
            } else {
                0
            }
        }

        /// The pending interrupt with the highest priority.
        fn iir_id(&self) -> u8 {
            if self.ier & IER_RLSI != 0 && self.line_status_pending() {
                IIR_RLSI
            } else if self.ier & IER_RDI != 0 && self.rx_data_pending() {
                IIR_RDI
            } else if self.ier & IER_RDI != 0 && self.rx_timeout_pending() {
                IIR_RX_TIMEOUT
            } else if self.ier & IER_THRI != 0 && self.thre_pending {
                IIR_THRI
            } else if self.ier & IER_MSI != 0 && self.msr_delta != 0 {
                IIR_MSI
            } else {
                IIR_NO_INT
            }
        }

        /// Updates the modem lines in the MSR and sets the change indicators.
        fn update_msr_lines(&mut self) {
            // In loopback mode, the modem outputs drive the modem inputs.
            let lines = if self.mcr & MCR_LOOP != 0 {
                let mut lines = 0;
                if self.mcr & MCR_RTS != 0 {
                    lines |= MSR_CTS;
                }
                if self.mcr & MCR_DTR != 0 {
                    lines |= MSR_DSR;
                }
                if self.mcr & MCR_OUT1 != 0 {
                    lines |= MSR_RI;
                }
                if self.mcr & MCR_OUT2 != 0 {
                    lines |= MSR_DCD;
                }
                lines
            } else {
                self.modem_inputs
            };
            let changed = lines ^ self.msr_lines;
            if changed & MSR_CTS != 0 {
                self.msr_delta |= MSR_DCTS;
            }
            if changed & MSR_DSR != 0 {
                self.msr_delta |= MSR_DDSR;
            }
            if changed & MSR_DCD != 0 {
                self.msr_delta |= MSR_DDCD;
            }
            // Only the trailing edge of RI is reported.
            if changed & MSR_RI != 0 && lines & MSR_RI == 0 {
                self.msr_delta |= MSR_TERI;
            }
            self.msr_lines = lines;
        }

        /// Applies an FCR write.
        fn write_fcr(&mut self, value: u8) {
            if !self.has_fifo {
                // A 16450 has no FCR.
                return;
            }
            let enable = value & FCR_ENABLE_FIFO != 0;
            if enable != self.fifo_enabled {
                // Changing the FIFO mode clears both FIFOs.
                self.rx.clear();
                self.tx.clear();
                self.fifo_enabled = enable;
            }
            // The other bits only take effect with the FIFO enabled.
            if enable {
                if value & FCR_CLEAR_RCVR != 0 {
                    self.rx.clear();
                }
                if value & FCR_CLEAR_XMIT != 0 {
                    self.tx.clear();
                    self.thre_pending = true;
                }
                // Bits 7:6 select the RX trigger level.
                self.rx_trigger = match value >> 6 {
                    0 => 1,
                    1 => 4,
                    2 => 8,
                    _ => 14,
                };
            }
        }

        /// Counts a register access and fails if the driver spins forever.
        fn budget(&mut self) {
            self.accesses += 1;
            assert!(
                self.accesses <= Self::ACCESS_BUDGET,
                "more than {} register accesses, the driver likely spins forever",
                Self::ACCESS_BUDGET
            );
        }
    }

    impl Backend for Device16550 {
        /// Reads a register, with the side effects of the real device.
        unsafe fn read(&mut self, register_offset: u8) -> u8 {
            self.budget();
            self.tick();
            let dlab = self.lcr & LCR_DLAB != 0;
            let value = match register_offset {
                DLL if dlab => self.dll,
                DLM if dlab => self.dlm,
                RX => {
                    if let Some(c) = self.rx.pop_front() {
                        self.top_errors_reported = false;
                        self.rx_idle_ticks = 0;
                        self.last_rx = c.data;
                    }
                    self.last_rx
                }
                IER => self.ier,
                IIR => {
                    let id = self.iir_id();
                    // Reading IIR acknowledges a THRE interrupt.
                    if id == IIR_THRI {
                        self.thre_pending = false;
                    }
                    id | self.iir_fifo_bits()
                }
                LCR => self.lcr,
                MCR => self.mcr,
                LSR => {
                    let lsr = self.lsr();
                    // Reading LSR clears the error bits.
                    self.overrun = false;
                    if !self.rx.is_empty() {
                        self.top_errors_reported = true;
                    }
                    lsr
                }
                MSR => {
                    let msr = self.msr_lines | self.msr_delta;
                    // Reading MSR clears the delta bits.
                    self.msr_delta = 0;
                    msr
                }
                SCR => self.scr,
                _ => panic!("invalid register offset {register_offset}"),
            };
            self.log.push((register_offset, false, value));
            value
        }

        /// Writes a register, with the side effects of the real device.
        unsafe fn write(&mut self, register_offset: u8, value: u8) {
            self.budget();
            self.tick();
            self.log.push((register_offset, true, value));
            let dlab = self.lcr & LCR_DLAB != 0;
            match register_offset {
                DLL if dlab => self.dll = value,
                DLM if dlab => self.dlm = value,
                TX => {
                    if self.tx.len() == self.fifo_capacity() {
                        self.tx_overflows += 1;
                        if !self.fifo_enabled {
                            // Without FIFO, the new byte overwrites THR.
                            self.tx[0] = value;
                        }
                    } else {
                        self.tx.push_back(value);
                    }
                    // Writing THR acknowledges a THRE interrupt.
                    self.thre_pending = false;
                }
                IER => {
                    let thri_enabled = self.ier & IER_THRI == 0 && value & IER_THRI != 0;
                    // Bits 7:4 are reserved and read as 0.
                    self.ier = value & 0x0f;
                    // Enabling the THRE interrupt with an empty THR raises it.
                    if thri_enabled && self.tx.is_empty() {
                        self.thre_pending = true;
                    }
                }
                FCR => self.write_fcr(value),
                LCR => self.lcr = value,
                MCR => {
                    // Bits 7:5 are reserved and read as 0.
                    self.mcr = value & 0x1f;
                    self.update_msr_lines();
                }
                // LSR and MSR are read-only on a 16550A.
                LSR | MSR => {}
                SCR => self.scr = value,
                _ => panic!("invalid register offset {register_offset}"),
            }
        }
    }
}

use model::*;

/// Creates a driver instance on top of a fresh device model.
fn new_uart() -> Uart16550<Device16550> {
    // SAFETY: The backend is a model; there is no real device.
    unsafe { Uart16550::new_with_backend(Device16550::new()) }
}

/// Raw register access to the model, like Linux' `serial_in()`.
fn serial_in(uart: &mut Uart16550<Device16550>, offset: u8) -> u8 {
    // SAFETY: The offsets used in this file are valid.
    unsafe { uart.backend_mut().read(offset) }
}

/// Raw register access to the model, like Linux' `serial_out()`.
fn serial_out(uart: &mut Uart16550<Device16550>, offset: u8, value: u8) {
    // SAFETY: The offsets used in this file are valid.
    unsafe { uart.backend_mut().write(offset, value) }
}

/// The model passes the probing of Linux' `autoconfig()` as a 16550A, so it
/// is a valid reference for the following tests.
#[test]
fn model_passes_linux_autoconfig() {
    let mut uart = new_uart();

    // IER read-back test.
    serial_out(&mut uart, IER, 0);
    assert_eq!(serial_in(&mut uart, IER) & 0x0f, 0);
    serial_out(&mut uart, IER, 0x0f);
    assert_eq!(serial_in(&mut uart, IER) & 0x0f, 0x0f);
    serial_out(&mut uart, IER, 0);

    // Loopback test: CTS follows RTS, DCD follows OUT2.
    serial_out(&mut uart, MCR, MCR_LOOP | MCR_OUT2 | MCR_RTS);
    assert_eq!(serial_in(&mut uart, MSR) & 0xf0, MSR_DCD | MSR_CTS);
    serial_out(&mut uart, MCR, 0);

    // FIFO detection.
    serial_out(&mut uart, FCR, FCR_ENABLE_FIFO);
    assert_eq!(serial_in(&mut uart, IIR) >> 6, 3, "should be a 16550A");
    serial_out(&mut uart, FCR, 0);
    assert_eq!(serial_in(&mut uart, IIR) >> 6, 0);

    // Scratch register.
    serial_out(&mut uart, SCR, 0xa5);
    assert_eq!(serial_in(&mut uart, SCR), 0xa5);
}

/// The model passes Linux' THRE tests in `serial8250_THRE_test()` and
/// `serial8250_iir_txen_test()`, so Linux wouldn't apply any workaround.
#[test]
fn model_passes_linux_thre_tests() {
    let mut uart = new_uart();

    // Enabling the THRE interrupt with an idle transmitter raises it, also
    // when enabling it again after an IIR read acknowledged it.
    serial_out(&mut uart, IER, IER_THRI);
    assert_eq!(serial_in(&mut uart, IIR), IIR_THRI);
    serial_out(&mut uart, IER, 0);
    serial_out(&mut uart, IER, IER_THRI);
    assert_eq!(serial_in(&mut uart, IIR), IIR_THRI);
    serial_out(&mut uart, IER, 0);

    serial_out(&mut uart, IER, IER_THRI);
    assert_ne!(serial_in(&mut uart, LSR) & LSR_TEMT, 0);
    assert_eq!(serial_in(&mut uart, IIR) & IIR_NO_INT, 0);
    serial_out(&mut uart, IER, 0);
}

/* ----- Startup ------------------------------------------------------------ */

/// The default configuration of Linux' serial console, see
/// `serial8250_console_setup()`: 9600 baud, 8N1, and no flow control. The FIFO
/// uses the RX trigger level Linux uses for a 16550A (`UART_FCR_R_TRIG_10`).
fn linux_config() -> Config {
    Config {
        interrupts: IER::empty(),
        frequency: CLK_FREQUENCY_HZ,
        prescaler_division_factor: None,
        fifo_trigger_level: Some(FifoTriggerLevel::Eight),
        baud_rate: BaudRate::Baud9600,
        data_bits: WordLength::EightBits,
        extra_stop_bits: false,
        parity: Parity::Disabled,
        check_cts_before_sending: false,
    }
}

/// Like [`linux_config`], but without FIFO, like on a 16450.
fn config_without_fifo() -> Config {
    Config {
        fifo_trigger_level: None,
        ..linux_config()
    }
}

/// After `init()`, the device is in the state Linux' `serial8250_do_startup()`
/// and `serial8250_do_set_termios()` leave it in.
#[test]
fn startup_leaves_device_configured() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();

    let dev = uart.backend();
    // 9600 baud at the standard 1.8432 MHz clock.
    assert_eq!(dev.divisor(), 12);
    assert_eq!(dev.peek(LCR), LCR_WLEN8, "8N1, DLAB cleared");
    assert!(dev.fifo_enabled());
    assert_eq!(dev.peek(IIR), IIR_FIFO_ENABLED | IIR_NO_INT);
    assert_eq!(dev.peek(MCR), MCR_DTR | MCR_RTS | MCR_OUT2);
    assert_eq!(dev.peek(IER), 0);
    assert_eq!(dev.peek(LSR), LSR_THRE | LSR_TEMT);
    assert!(!dev.irq());
}

/// Like Linux, `init()` discards data received before, e.g., while an earlier
/// boot stage used the device.
#[test]
fn startup_discards_stale_rx_data() {
    let mut uart = new_uart();
    uart.backend_mut().inject_rx(b"stale");
    uart.init(linux_config()).unwrap();

    assert_eq!(uart.backend().rx_level(), 0);
    assert!(uart.try_receive_byte().is_err());
}

/// Like [`startup_discards_stale_rx_data`], but without FIFO. FCR can't clear
/// anything then, so the data must be read out, as Linux does by reading RX.
#[test]
fn startup_discards_stale_rx_data_without_fifo() {
    let mut uart = new_uart();
    uart.backend_mut().inject_rx(b"x");
    uart.init(config_without_fifo()).unwrap();

    assert_eq!(uart.backend().rx_level(), 0);
    assert!(uart.try_receive_byte().is_err());
}

/// Like Linux, `init()` clears pending line and modem status conditions, so
/// enabling interrupts doesn't raise stale ones.
#[test]
fn startup_clears_stale_interrupt_conditions() {
    let mut uart = new_uart();
    let dev = uart.backend_mut();
    // An overrun from before, in the 1-byte RBR of the reset device, and a
    // modem line change.
    dev.inject_rx(&[0; 2]);
    dev.set_modem_inputs(MSR_CTS | MSR_DSR);

    let config = Config {
        interrupts: IER::DATA_READY | IER::RECEIVER_LINE_STATUS | IER::MODEM_STATUS,
        ..linux_config()
    };
    uart.init(config).unwrap();

    let dev = uart.backend();
    assert_eq!(dev.peek(IER), IER_RDI | IER_RLSI | IER_MSI);
    assert_eq!(dev.peek(IIR), IIR_FIFO_ENABLED | IIR_NO_INT);
    assert!(!dev.irq());
}

/// An invalid baud rate is reported, and the device stays usable: DLAB is
/// clear, so offsets 0 and 1 still address the data registers and IER.
#[test]
fn startup_with_invalid_baud_rate_keeps_dlab_clear() {
    let mut uart = new_uart();
    let config = Config {
        baud_rate: BaudRate::Custom(7211),
        ..linux_config()
    };

    assert!(uart.init(config).is_err());
    assert_eq!(uart.backend().peek(LCR) & LCR_DLAB, 0);
}

/// A baud rate too low for the 16-bit divisor is reported as error.
#[test]
fn startup_with_too_low_baud_rate_fails_gracefully() {
    let mut uart = new_uart();
    let config = Config {
        baud_rate: BaudRate::Custom(1),
        ..linux_config()
    };

    assert!(uart.init(config).is_err());
}

/// The presence check doesn't destroy the scratch register, like Linux' 8250
/// scratch test, which restores it.
#[test]
fn startup_preserves_scratch_register() {
    let mut uart = new_uart();
    serial_out(&mut uart, SCR, 0x5a);

    uart.init(linux_config()).unwrap();

    assert_eq!(uart.backend().peek(SCR), 0x5a);
}

/// Like Linux' `autoconfig()`, `init()` detects whether the FIFOs are really
/// enabled, e.g., not on a 16450, which has none.
#[test]
fn startup_detects_fifo() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    assert!(uart.fifo_enabled());

    // SAFETY: The backend is a model; there is no real device.
    let mut uart = unsafe { Uart16550::new_with_backend(Device16550::new_16450()) };
    uart.init(linux_config()).unwrap();
    assert!(!uart.fifo_enabled());
}

/* ----- Self test ---------------------------------------------------------- */

/// The loopback test passes and leaves no traces: no data on the wire, no
/// received data, and the previous MCR.
#[test]
fn loopback_test_leaves_no_traces() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    let mcr = uart.backend().peek(MCR);

    uart.test_loopback().unwrap();

    let dev = uart.backend_mut();
    assert_eq!(dev.peek(MCR), mcr);
    assert!(dev.take_sent().is_empty());
    assert_eq!(dev.rx_level(), 0);
    assert_eq!(dev.tx_overflows, 0);
}

/// The loopback test also works without FIFO.
#[test]
fn loopback_test_without_fifo() {
    let mut uart = new_uart();
    uart.init(config_without_fifo()).unwrap();

    uart.test_loopback().unwrap();

    let dev = uart.backend();
    assert_eq!(dev.tx_overflows, 0);
    assert_eq!(dev.peek(MCR), MCR_DTR | MCR_RTS | MCR_OUT2);
}

/// A failing loopback test leaves the device usable: it reports the error and
/// leaves loopback mode again.
#[test]
fn loopback_test_failure_leaves_loopback_mode() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    uart.backend_mut().loopback_xor = 0xff;

    assert!(uart.test_loopback().is_err());

    assert_eq!(uart.backend().peek(MCR) & MCR_LOOP, 0);
}

/// The loopback test doesn't leave a modem status change behind, which would
/// raise a spurious modem status interrupt. In loopback mode, the modem lines
/// follow MCR instead of the remote, which asserts CTS and DSR here.
#[test]
fn loopback_test_leaves_no_modem_status_change() {
    let mut uart = new_uart();
    uart.backend_mut().set_modem_inputs(MSR_CTS | MSR_DSR);
    uart.init(Config {
        interrupts: IER::MODEM_STATUS,
        ..linux_config()
    })
    .unwrap();

    uart.test_loopback().unwrap();

    assert!(!uart.backend().irq());
}

/// On a 16450, the loopback test also works with a config that enables the
/// FIFO, which the device doesn't have.
#[test]
fn loopback_test_on_16450() {
    // SAFETY: The backend is a model; there is no real device.
    let mut uart = unsafe { Uart16550::new_with_backend(Device16550::new_16450()) };
    uart.init(linux_config()).unwrap();

    uart.test_loopback().unwrap();

    assert_eq!(uart.backend().tx_overflows, 0);
}

/// The loopback test keeps modem status changes that happened before, so
/// `msr()` still reports them.
#[test]
fn loopback_test_keeps_modem_status_changes() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    uart.backend_mut().set_modem_inputs(MSR_CTS | MSR_DSR);

    uart.test_loopback().unwrap();

    assert!(uart.msr().contains(MSR::DELTA_CTS | MSR::DELTA_DSR));
}

/// The loopback test disables interrupts while it sends data, so that no
/// interrupt handler takes the looped-back data, and restores them afterwards.
#[test]
fn loopback_test_disables_interrupts_meanwhile() {
    let interrupts = IER::DATA_READY | IER::RECEIVER_LINE_STATUS;
    let mut uart = new_uart();
    uart.init(Config {
        interrupts,
        ..linux_config()
    })
    .unwrap();
    uart.backend_mut().log.clear();

    uart.test_loopback().unwrap();

    let dev = uart.backend();
    // Replay the logged writes to know IER whenever looped-back data is sent.
    let mut ier = interrupts.bits();
    let mut loopback = false;
    for &(offset, is_write, value) in &dev.log {
        match (offset, is_write) {
            (IER, true) => ier = value,
            (MCR, true) => loopback = value & MCR_LOOP != 0,
            (TX, true) if loopback => assert_eq!(ier, 0, "interrupts enabled"),
            _ => {}
        }
    }
    assert_eq!(dev.peek(IER), interrupts.bits());
}

/// The loopback test lets a running transmission finish first, so it is
/// neither cut off nor looped back.
#[test]
fn loopback_test_lets_transmission_finish() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    uart.send_bytes_exact(b"hello");

    uart.test_loopback().unwrap();

    assert_eq!(uart.backend_mut().take_sent(), b"hello");
}

/* ----- Polled I/O --------------------------------------------------------- */

/// Lets the transmitter drain and returns what went out on the wire.
fn drain_sent(uart: &mut Uart16550<Device16550>) -> Vec<u8> {
    let dev = uart.backend_mut();
    dev.idle((FIFO_SIZE + 1) * Device16550::CHAR_TICKS);
    dev.take_sent()
}

/// Polled transmit, like the Linux console: every byte goes out in order, and
/// the TX FIFO never overflows.
#[test]
fn polled_transmit() {
    let message: Vec<u8> = (0..100).collect();
    for config in [linux_config(), config_without_fifo()] {
        let mut uart = new_uart();
        uart.init(config).unwrap();

        uart.send_bytes_exact(&message);

        assert_eq!(drain_sent(&mut uart), message);
        assert_eq!(uart.backend().tx_overflows, 0);
    }
}

/// Polled receive, like Linux' `serial8250_get_poll_char()`: no data is
/// reported as such, and available data is returned in order.
#[test]
fn polled_receive() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    assert!(uart.try_receive_byte().is_err());

    uart.backend_mut().inject_rx(b"hello");
    let mut buf = [0; 16];
    let n = uart.receive_bytes(&mut buf);

    assert_eq!(&buf[..n], b"hello");
    assert!(uart.try_receive_byte().is_err());
}

/// Like Linux' `serial8250_rx_chars()`, a driver can see line errors of a
/// received byte in LSR before reading it.
#[test]
fn receive_errors_are_visible_in_lsr() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    uart.backend_mut().inject_rx_with_errors(b'x', LSR_PE);

    assert!(uart.lsr().contains(LSR::PARITY_ERROR));
    assert_eq!(uart.try_receive_byte(), Ok(b'x'));
}

/// An RX FIFO overrun is reported once in LSR, and the data received before
/// is kept.
#[test]
fn receive_overrun() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    let data: Vec<u8> = (0..=FIFO_SIZE as u8).collect();
    uart.backend_mut().inject_rx(&data);

    assert!(uart.lsr().contains(LSR::OVERRUN_ERROR));
    assert!(!uart.lsr().contains(LSR::OVERRUN_ERROR));
    let mut buf = [0; FIFO_SIZE + 1];
    let n = uart.receive_bytes(&mut buf);
    assert_eq!(&buf[..n], &data[..FIFO_SIZE]);
}

/// On a 16450, which has no FIFO, transmitting doesn't lose data. Linux
/// detects the missing FIFO via IIR bits 7:6 and sends one byte per THRE.
#[test]
fn transmit_on_16450() {
    // SAFETY: The backend is a model; there is no real device.
    let mut uart = unsafe { Uart16550::new_with_backend(Device16550::new_16450()) };
    uart.init(linux_config()).unwrap();
    let message: Vec<u8> = (0..100).collect();

    uart.send_bytes_exact(&message);

    assert_eq!(uart.backend().tx_overflows, 0);
    assert_eq!(drain_sent(&mut uart), message);
}

/// Transmitting reads LSR, which clears the error bits of received data. Like
/// Linux, which saves them in `lsr_saved_flags`, the driver must not lose them.
#[test]
fn transmit_keeps_receive_errors() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    uart.backend_mut().inject_rx_with_errors(b'x', LSR_FE);

    uart.send_bytes_exact(b"y");

    assert!(uart.lsr().contains(LSR::FRAMING_ERROR));
}

/// Receiving reads LSR as well, and also must not lose the errors of the
/// received byte.
#[test]
fn receive_keeps_receive_errors() {
    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();
    uart.backend_mut().inject_rx_with_errors(b'x', LSR_FE);

    assert_eq!(uart.try_receive_byte(), Ok(b'x'));

    assert!(uart.lsr().contains(LSR::FRAMING_ERROR));
}

/// Like Linux' `tcdrain()`, which waits for TEMT, flushing waits until all
/// data went out on the wire.
#[cfg(feature = "embedded-io")]
#[test]
fn flush_waits_for_transmitter() {
    use embedded_io::Write;

    let mut uart = new_uart();
    uart.init(linux_config()).unwrap();

    uart.write_all(b"hello").unwrap();
    uart.flush().unwrap();

    assert_eq!(uart.backend_mut().take_sent(), b"hello");
}
