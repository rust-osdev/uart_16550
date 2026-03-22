// SPDX-License-Identifier: MIT OR Apache-2.0

//! # uart_16550
//!
//! Simple yet highly configurable low-level driver for
//! [16550 UART devices][uart], typically known and used as serial ports or
//! COM ports. Easy integration into Rust while providing fine-grained control
//! where needed (e.g., for kernel drivers).
//!
//! The "serial device" or "COM port" in typical x86 machines is almost always
//! backed by a **16550 UART devices**, may it be physical or emulated. This
//! crate offers convenient and powerful abstractions for these devices, and
//! also works for other architectures, such as ARM or RISC-V, by offering
//! support for MMIO-mapped devices.
//!
//! Serial ports are especially useful for debugging or operating system
//! learning projects. See [`Uart16550`] to get started.
//!
//! ## Features
//!
//! - ✅ Full configure, transmit, receive, and interrupt support for UART
//!   16550–compatible devices
//! - ✅ High-level, ergonomic abstractions and types paired with support for
//!   plain integers
//! - ✅ Very easy to integrate, highly configurable when needed
//! - ✅ Validated on **real hardware** as well as across different virtual
//!   machines
//! - ✅ Fully type-safe and derived directly from the official
//!   [specification][uart]
//! - ✅ Supports both **x86 port-mapped I/O** and **memory-mapped I/O** (MMIO)
//! - ✅ `no_std`-compatible and allocation-free by design
//!
//! ## Focus, Scope & Limitations
//!
//! While serial ports are often used in conjunction with VT102-like terminal
//! emulation, the primary focus of this crate is strict specification
//! compliance and convenient direct access to the underlying hardware for
//! transmitting and receiving bytes, including all necessary device
//! configuration.
//!
//! For basic terminal-related functionality, such as newline normalization and
//! backspace handling, we provide [`Uart16550Tty`] as a **basic** convenience
//! layer.
//!
//! # Overview
//!
//! Use [`Uart16550Tty`] for a quick start. For more fine-grained low-level
//! control, please have a look at [`Uart16550`] instead.
//!
//! # Example (Minimalistic)
//!
//! ```rust,no_run
//! use uart_16550::{Config, Uart16550Tty};
//! use core::fmt::Write;
//!
//! // SAFETY: The address is valid and we have exclusive access.
//! let mut uart = unsafe { Uart16550Tty::new_mmio(0x1000 as *mut _, 4, Config::default()).expect("should initialize device") };
//! //                                    ^ or `new_port(0x3f8, Config::default())`
//! uart.write_str("hello world\nhow's it going?");
//! ```
//!
//! See [`Uart16550Tty`] for more details.
//!
//! # Example (More low-level control)
//!
//! ```rust,no_run
//! use uart_16550::{Config, Uart16550};
//!
//! // SAFETY: The address is valid and we have exclusive access.
//! let mut uart = unsafe { Uart16550::new_mmio(0x1000 as *mut _, 4).expect("should be valid port") };
//! //                                 ^ or `new_port(0x3f8)`
//! uart.init(Config::default()).expect("should init device successfully");
//! uart.test_loopback().expect("should have working loopback mode");
//! uart.check_connected().expect("should have physically connected receiver");
//! uart.send_bytes_exact(b"hello world!");
//! ```
//!
//! See [`Uart16550`] for more details.
//!
//! # Testing on Real Hardware
//!
//! ## Establish a Serial Connection and Test Using Linux
//!
//! You need two machines, one must have a physical COM1 port. In this example,
//! we're using Linux.
//!
//! Connect your COM1 port to another computer. You will need:
//!
//! - Cable: COM1 pin-out to DE9 (RS-232)
//! - Cable: DE-9 (RS-232) to USB Serial
//! - Null modem component (can be a cable or adapter): This enables
//!   point-to-point communication by crossing the RX and TX lines of both
//!   communication partners.
//!
//! Test serial connection works:
//! - Machine 1: `$ sudo minicom -D /dev/ttyS0`
//! - Machine 2: `$ sudo minicom -D /dev/ttyUSB0`
//!
//! Most likely, both Linux machines will use the default baud rate of
//! [`BaudRate::Baud9600`]. If not, boot the Linux machines with an
//! [updated command line][linux-serial-console-doc].
//!
//! If you can send data between both parties, you can proceed with the next
//! step.
//!
//! ## Test This Driver
//!
//! Build your own (mini) operating system using this driver and check if you
//! can receive data from Machine 2 (see above) or send data to it.
//!
//! A relatively easy and flexible approach is to build a UEFI application and
//! copy the resulting EFI file onto a USB stick with a bootable partition
//! to path `EFI\BOOT\BOOTX64.EFI`.
//!
//! The workflow with `minicom` on Machine 2 is the same.
//!
//!
//! [linux-serial-console-doc]: https://docs.kernel.org/admin-guide/serial-console.html
//! [uart]: https://en.wikipedia.org/wiki/16550_UART

#![no_std]
#![deny(
    clippy::all,
    clippy::cargo,
    clippy::nursery,
    clippy::must_use_candidate,
    clippy::missing_safety_doc,
    clippy::undocumented_unsafe_blocks,
    clippy::needless_pass_by_value
)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(rustdoc::all)]

#[cfg(test)]
extern crate std;

pub use crate::config::*;
pub use crate::error::*;
pub use crate::tty::*;

use crate::backend::{Backend, MmioAddress, MmioBackend};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::{PioBackend, PortIoAddress};
use crate::spec::registers::{DLL, DLM, FCR, IER, ISR, LCR, LSR, MCR, MSR, SPR, offsets};
use crate::spec::{FIFO_SIZE, NUM_REGISTERS, calc_baud_rate, calc_divisor};
use core::cmp;
use core::hint;
use core::num::NonZeroU8;

pub mod backend;
pub mod spec;

mod config;
#[cfg(feature = "embedded-io")]
mod embedded_io;
mod error;
mod tty;

/// Powerful abstraction over a [16550 UART device][uart] with access to
/// low-level details paired with strong flexibility for higher-level layers.
///
/// All reads and writes involving device register from/to that device operate
/// on the underlying hardware.
///
/// This type is generic over x86 port I/O and MMIO via the corresponding
/// constructors (`Uart16550::new_port()` and [`Uart16550::new_mmio()`].
///
/// # Example (Minimal)
///
/// ```rust,no_run
/// use uart_16550::{Config, Uart16550};
///
/// // SAFETY: The address is valid and we have exclusive access.
/// let mut uart = unsafe { Uart16550::new_mmio(0x1000 as *mut _, 4).unwrap() };
/// //                                 ^ or `new_port(0x3f8)`
/// uart.init(Config::default()).expect("should init device successfully");
/// uart.send_bytes_exact(b"hello world!");
/// ```
///
/// # Example (Recommended)
///
/// ```rust,no_run
/// use uart_16550::{Config, Uart16550};
///
/// // SAFETY: The address is valid and we have exclusive access.
/// let mut uart = unsafe { Uart16550::new_mmio(0x1000 as *mut _, 4).expect("should be valid port") };
/// //                                 ^ or `new_port(0x3f8)`
/// uart.init(Config::default()).expect("should init device successfully");
/// uart.test_loopback().expect("should have working loopback mode");
/// uart.check_connected().expect("should have physically connected receiver");
/// uart.send_bytes_exact(b"hello world!");
/// ```
///
/// # Sending and Receiving Data
///
/// The API provides both **non-blocking (`try_*`)** and
/// **blocking (`*_exact`)** variants for transmitting and receiving data:
///
/// ## Non-blocking
///
/// - [`Uart16550::try_send_byte`]: Attempt to transmit a single byte.
/// - [`Uart16550::send_bytes`]: Transmit multiple bytes without blocking,
///   returning the number of bytes written.
/// - [`Uart16550::try_receive_byte`]: Attempt to receive a single byte.
/// - [`Uart16550::receive_bytes`]: Read bytes into a buffer without blocking,
///   returning the number of bytes read.
///
/// These methods return immediately if the hardware can't complete the
/// operation.
///
/// ## Blocking
///
/// - [`Uart16550::send_bytes_exact`]: Transmit all bytes, looping until the
///   entire buffer has been written.
/// - [`Uart16550::receive_bytes_exact`]: Receive bytes until the provided
///   buffer is completely filled.
///
/// These methods spin until completion.
///
/// # MMIO and Port I/O
///
/// Uart 16550 devices are typically mapped via port I/O on x86 and via MMIO on
/// other platforms. The constructors `new_port()` and `new_mmio()` create an
/// instance of a device with the corresponding backend.
///
/// # Hints for Usage on Real Hardware
///
/// Please note that real hardware often behaves quite differently. Just because
/// this works in a VM (e.g., Cloud Hypervisor and VMM), your application
/// doesn't necessarily work on real hardware. You might need to fiddle with
/// the configuration or perform more research for potential hardware quirks
/// of your system.
///
/// [uart]: https://en.wikipedia.org/wiki/16550_UART
#[derive(Debug)]
pub struct Uart16550<B: Backend> {
    backend: B,
    base_address: B::Address,
    // The currently active config.
    config: Config,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Uart16550<PioBackend> {
    /// Creates a new [`Uart16550`] backed by x86 port I/O.
    ///
    /// # Arguments
    ///
    /// - `base_address`: Base address of the UART.
    ///
    /// # Safety
    ///
    /// Callers must ensure that the base port is valid and safe to use for the
    /// **whole lifetime** of the device. Further, all [`NUM_REGISTERS`]
    /// registers must be safely reachable from the base address.
    pub unsafe fn new_port(base_port: u16) -> Result<Self, InvalidAddressError<PortIoAddress>> {
        let base_address = PortIoAddress(base_port);
        if base_port.checked_add(NUM_REGISTERS as u16 - 1).is_none() {
            return Err(InvalidAddressError::InvalidBaseAddress(base_address));
        }

        let backend = PioBackend(base_address);

        Ok(Self {
            backend,
            base_address,
            // Will be replaced by the actual config in init() afterwards.
            config: Config::default(),
        })
    }
}

impl Uart16550<MmioBackend> {
    /// Creates a new [`Uart16550`] backed by MMIO.
    ///
    /// # Arguments
    ///
    /// - `base_address`: Base address of the UART.
    /// - `stride`: The stride is the fixed byte distance in physical address
    ///   space between consecutive logical registers, i.e. how much the address
    ///   increases when moving from one register index to the next. Typical
    ///   values are `1`, `2`, `4`, and `8` - depending on your hardware/board.
    ///
    /// # Safety
    ///
    /// Callers must ensure that the base address is valid and safe to use for
    /// the **whole lifetime** of the device. Further, all [`NUM_REGISTERS`]
    /// registers must be safely reachable from the base address.
    pub unsafe fn new_mmio(
        base_address: *mut u8,
        stride: u8,
    ) -> Result<Self, InvalidAddressError<MmioAddress>> {
        let base_address = MmioAddress(base_address);
        if base_address.0.is_null() {
            return Err(InvalidAddressError::InvalidBaseAddress(base_address));
        }

        if stride == 0 || !stride.is_power_of_two() {
            return Err(InvalidAddressError::InvalidStride(stride));
        }

        if (base_address.0 as usize)
            .checked_add((NUM_REGISTERS - 1) * stride as usize)
            .is_none()
        {
            return Err(InvalidAddressError::InvalidBaseAddress(base_address));
        }

        // Compiler will optimize the unwrap away
        let stride = NonZeroU8::new(stride).unwrap();

        let backend = MmioBackend {
            base_address,
            stride,
        };

        Ok(Self {
            backend,
            base_address,
            // Will be replaced by the actual config in init() afterwards.
            config: Config::default(),
        })
    }
}

impl<B: Backend> Uart16550<B> {
    /* ----- Init, Setup, Tests --------------------------------------------- */

    /// Initializes the devices according to the provided [`Config`] including a
    /// few typical as well as opinionated settings.
    ///
    /// This function also tries to detect if the UART is present at all by
    /// writing a byte to the [`SPR`] register and read it back afterwards.
    ///
    /// It is **recommended** to call [`Self::test_loopback`] next to check that
    /// the device works. Further, a call to [`Self::check_connected`] helps to
    /// detect if a remote is connected.
    ///
    /// # Caution
    ///
    /// Callers must ensure that using this type with the underlying hardware
    /// is done only in a context where such operations are valid and safe
    /// (e.g., you have exclusive device access).
    ///
    /// Further, the serial config must match the expectations of the receiver
    /// on the other side. Otherwise, garbage will be received.
    pub fn init(&mut self, config: Config) -> Result<(), InitError> {
        // It is important to set this early as some helpers rely on that.
        self.config = config;

        // SPR test: write something and try to read it again.
        // => detect if UART16550 is there
        {
            let mut check_fn = |write| {
                // SAFETY: We operate on valid register addresses.
                let read = unsafe {
                    self.backend.write(offsets::SPR as u8, write);
                    self.backend.read(offsets::SPR as u8)
                };

                if read != write {
                    return Err(InitError::DeviceNotPresent);
                }

                Ok(())
            };

            check_fn(0x42)?;
            check_fn(0x73)?;
        }

        // Disable all interrupts (for now).
        // SAFETY: We operate on valid register addresses.
        unsafe {
            self.backend.write(offsets::IER as u8, 0);
        }

        // Set baud rate.
        // SAFETY: We operate on valid register addresses.
        unsafe {
            // Set Divisor Latch Access Bit (DLAB) to access DLL and DLM next
            self.backend.write(offsets::LCR as u8, LCR::DLAB.bits());

            let divisor = calc_divisor(
                self.config.frequency,
                self.config.baud_rate.to_integer(),
                self.config.prescaler_division_factor,
            )
            .map_err(InitError::InvalidBaudRate)?;

            let low = (divisor & 0xff) as u8;
            let high = ((divisor >> 8) & 0xff) as u8;
            self.backend.write(offsets::DLL as u8, low);
            self.backend.write(offsets::DLM as u8, high);

            // Clear DLAB
            self.backend.write(offsets::LCR as u8, 0);
        }

        // Set line control register.
        // SAFETY: We operate on valid register addresses.
        unsafe {
            let mut lcr = LCR::from_bits_retain(0);
            lcr = lcr.set_word_length(self.config.data_bits);
            if self.config.extra_stop_bits {
                lcr |= LCR::MORE_STOP_BITS;
            }
            lcr = lcr.set_parity(self.config.parity);
            // don't set break
            // don't set DLAB
            self.backend.write(offsets::LCR as u8, lcr.bits());
        }

        // SAFETY: We have exclusive access to the device.
        unsafe {
            self.configure_fcr();
        }

        // Set modem control register.
        // SAFETY: We operate on valid register addresses.
        unsafe {
            let mut mcr = MCR::from_bits_retain(0);
            // signal that we are powered on and configured
            // (assert MSR::DSR on remote)
            mcr |= MCR::DTR;
            // signal that we are ready to receive data
            // (assert MSR::CTS on remote)
            mcr |= MCR::RTS;
            // enable interrupt routing to the interrupt controller
            // (so far individual interrupts are still disabled in IER)
            mcr |= MCR::OUT_2_INT_ENABLE;

            self.backend.write(offsets::MCR as u8, mcr.bits());
        }

        // Set interrupts.
        // SAFETY: We operate on valid register addresses.
        unsafe {
            self.backend
                .write(offsets::IER as u8, self.config.interrupts.bits());
        }

        // In case there is anything in THR, THR's FIFO or TSR (for
        // example because the device was already initialized by another
        // driver), we wait for the data to be drained. This way, we can ensure
        // that on real hardware, the first bits of the payload don't get
        // corrupted.
        //
        // Note that drain time depends on the configured baud rate - at low
        // baud rates (e.g. 9600) this may block for up to ~16 ms if the FIFO is
        // full.
        //
        // The data is also drained if no one is connected to the port, so we
        // can't get stuck here.
        loop {
            let lsr = self.lsr();
            if lsr.contains(LSR::TRANSMITTER_EMPTY) {
                break;
            } else {
                hint::spin_loop()
            }
        }
        Ok(())
    }

    /// Tests the device in loopback mode.
    ///
    /// It is **recommended** to call this function **after** [`Self::init`].
    ///
    /// # Caution
    ///
    /// - No other data **must** be send over the device while this test isn't
    ///   finished
    /// - The FIFO will be re-configured and activated by this test.
    // NEVER CHANGE this function without testing on real hardware!
    pub fn test_loopback(&mut self) -> Result<(), LoopbackError> {
        /// Single test byte. Chosen arbitrarily.
        const TEST_BYTE: u8 = 0x42;
        /// Test message. Must be smaller than [`FIFO_SIZE`].
        const TEST_MESSAGE: [u8; FIFO_SIZE] = *b"hello world!1337";

        // SAFETY: We operate on valid register addresses.
        unsafe {
            let old_mcr = self.mcr();

            // We also disable interrupts here.
            self.backend
                .write(offsets::MCR as u8, MCR::LOOP_BACK.bits());

            // Reset send and receive FIFOs.
            self.configure_fcr();

            // Drain any data that might be still there
            while self.receive_bytes(&mut [0]) > 0 {}

            // First: check a single byte
            {
                self.try_send_byte(TEST_BYTE)
                    .map_err(LoopbackError::SendError)?;

                let mut read_buf = [0];
                // Tests on real hardware showed that there can be a short delay
                // until we can read the data. Therefore, we use the blocking
                // API rather than `try_receive_byte()`.
                self.receive_bytes_exact(&mut read_buf);
                let read = read_buf[0];
                if read != TEST_BYTE {
                    return Err(LoopbackError::UnexpectedLoopbackByte {
                        expected: TEST_BYTE,
                        actual: read,
                    });
                }
            }

            // Now check sending and reading a whole message.
            // This requires the FIFO to be activated.
            {
                self.send_bytes_exact(&TEST_MESSAGE);

                let mut read_buffer = [0_u8; TEST_MESSAGE.len()];
                // Tests on real hardware showed that there can be a short delay
                // until we can read the data. Therefore, we use the blocking
                // API rather than `receive_bytes()`.
                self.receive_bytes_exact(&mut read_buffer);
                let read = read_buffer;

                if read != TEST_MESSAGE {
                    return Err(LoopbackError::UnexpectedLoopbackMsg {
                        expected: TEST_MESSAGE,
                        actual: read_buffer,
                    });
                }
            }

            // restore MCR
            self.backend.write(offsets::MCR as u8, old_mcr.bits());
        }

        Ok(())
    }

    /// Performs some checks to see if the UART is connected to a physical
    /// device and **can receive data from us**.
    ///
    /// This function is supposed to be called before transmitting or receiving
    /// data. Once this check succeeds, the connection is established and ready.
    /// A [`InterruptType::ModemStatus`] may indicate that this check needs to
    /// be performed again.
    ///
    /// # Hints for Real Hardware
    ///
    /// Please note that some cables (especially when a Null modem is included)
    /// never raise the CD or even DSR line. So even if this checks fails,
    /// connections might work.
    ///
    /// [`InterruptType::ModemStatus`]: crate::spec::registers::InterruptType::ModemStatus
    pub fn check_connected(&mut self) -> Result<(), RemoteReadyToReceiveError> {
        // SAFETY: We operate on valid register addresses.
        let msr = unsafe { self.backend.read(offsets::MSR as u8) };
        let msr = MSR::from_bits_retain(msr);
        // DSR reflects the remote's MCR::DTR. Not asserted means the remote
        // is absent, unpowered, or uninitialized.
        if !msr.contains(MSR::DSR) {
            return Err(RemoteReadyToReceiveError::NoRemoteConnectedNoDSR);
        }

        /* This doesn't work on real hardware (in my case). Probably, checking
           DSR is sufficient! Some research also tells to just check DSR, at
           least for point-to-point connections in Null-modem mode.
        // CD is asserted by our local modem upon detecting a valid carrier.
        // In null-modem wiring, CD is typically looped from the remote's DTR.
        // Not asserted means the line is not live or no carrier has been
        // established.
        if !msr.contains(MSR::CD) {
            return Err(RemoteReadyToReceiveError::NoRemoteConnectedNoCD);
        }*/

        // Did remote set/assert MCR::RTS?
        if !msr.contains(MSR::CTS) {
            return Err(RemoteReadyToReceiveError::RemoteNotClearToSend);
        }
        Ok(())
    }

    /// Checks if there is at least one pending byte on the device that can be
    /// read.
    ///
    /// Please note that it is not required to call this before any of the
    /// receive-methods, as all of them perform this check also internally
    /// already.
    ///
    /// This differs from [`Self::check_connected`] as it only checks the
    /// internal in-buffer without checking for an established connection.
    pub fn ready_to_receive(&mut self) -> Result<(), ByteReceiveError> {
        let lsr = self.lsr();

        if !lsr.contains(LSR::DATA_READY) {
            return Err(ByteReceiveError);
        }

        Ok(())
    }

    /// Determines if data can be sent.
    ///
    /// Please note that it is not required to call this before any of the
    /// send-methods, as all of them perform this check also internally
    /// already.
    ///
    /// This differs from [`Self::check_connected`] as it only checks if further
    /// data can be written (e.g., internal FIFO is empty) without checking for
    /// an established connection.
    pub fn ready_to_send(&mut self) -> Result<(), ByteSendError> {
        let lsr = self.lsr();
        let msr = self.msr();
        let mcr = self.mcr();

        // In FIFO mode, this bit is set when the transmitter’s FIFO is
        // completely empty, being 0 if there is at least one byte in the
        // FIFO waiting to be passed to the TSR for transmission. In Non-FIFO
        // mode we must return on error to prevent data corruption in THR.
        if !lsr.contains(LSR::THR_EMPTY) {
            return Err(ByteSendError::NoCapacity);
        }

        // Software flow control. TODO, what to do with hardware flow control?
        // Is this something we can and should support?
        if !mcr.contains(MCR::LOOP_BACK) && !msr.contains(MSR::CTS) {
            return Err(ByteSendError::RemoteNotClearToSend);
        }

        Ok(())
    }

    /* ----- User I/O ------------------------------------------------------- */

    /// Tries to read a raw byte from the device.
    ///
    /// This will receive whatever a remote has sent to us.
    pub fn try_receive_byte(&mut self) -> Result<u8, ByteReceiveError> {
        self.ready_to_receive()?;

        // SAFETY: We operate on valid register addresses.
        let byte = unsafe { self.backend.read(offsets::DATA as u8) };

        Ok(byte)
    }

    /// Tries to write a raw byte to the device.
    ///
    /// This will be transmitted to the remote.
    #[inline]
    pub fn try_send_byte(&mut self, byte: u8) -> Result<(), ByteSendError> {
        // bytes are typically written in chunks for higher performance,
        // therefore `send_bytes()` is our base here.
        match self.send_bytes(&[byte]) {
            0 => Err(ByteSendError::NoCapacity),
            _ => Ok(()),
        }
    }

    /// Reads bytes from the device into the provided buffer.
    ///
    /// Returns the number of bytes actually read, which may be less than
    /// `buffer.len()` if fewer bytes are available. Returns `0` if no data is
    /// currently available.
    ///
    /// Call repeatedly with a shifted buffer slice to receive all expected data.
    pub fn receive_bytes(&mut self, buffer: &mut [u8]) -> usize {
        buffer
            .iter_mut()
            .map_while(|slot: &mut u8| {
                self.try_receive_byte().ok().map(|byte| {
                    *slot = byte;
                })
            })
            .count()
    }

    /// Sends bytes to the remote without blocking.
    ///
    /// Returns the number of bytes actually written: `0` if no data can
    /// currently be written, `1` in non-FIFO mode, or up to [`FIFO_SIZE`]
    /// in FIFO mode.
    ///
    /// Call repeatedly with a shifted buffer slice to send all data.
    pub fn send_bytes(&mut self, buffer: &[u8]) -> usize {
        if buffer.is_empty() {
            return 0;
        }

        if self.ready_to_send().is_err() {
            return 0;
        }

        let fifo_enabled = self.config.fifo_trigger_level.is_some();
        let bytes = if fifo_enabled {
            let max_index = cmp::min(FIFO_SIZE, buffer.len());
            &buffer[..max_index]
        } else {
            &buffer[..1]
        };

        // Spec: According to spec, it is fine to send multiple bytes in a row
        // in FIFO mode to the data register (THR).
        for &byte in bytes {
            // SAFETY: We operate on valid register addresses.
            unsafe {
                self.backend.write(offsets::DATA as u8, byte);
            }
        }

        bytes.len()
    }

    /// Similar to [`Self::receive_bytes`] but loops until enough bytes were
    /// read to fully fill the buffer.
    ///
    /// Beware that this can spin indefinitely.
    pub fn receive_bytes_exact(&mut self, buffer: &mut [u8]) {
        for slot in buffer {
            // Loop until we can fill the slot.
            loop {
                if let Ok(byte) = self.try_receive_byte() {
                    *slot = byte;
                    break;
                } else {
                    hint::spin_loop()
                }
            }
        }
    }

    /// Similar to [`Self::send_bytes`] but loops until all bytes were
    /// written entirely to the remote.
    ///
    /// Beware that this can spin indefinitely.
    pub fn send_bytes_exact(&mut self, bytes: &[u8]) {
        let mut remaining_bytes = bytes;
        while !remaining_bytes.is_empty() {
            let n = self.send_bytes(remaining_bytes);
            remaining_bytes = &remaining_bytes[n..];

            if n > 0 {
                continue;
            } else {
                hint::spin_loop()
            }
        }
    }

    /* ----- Typed Register Getters ----------------------------------------- */

    /// Fetches the current value from the [`IER`].
    pub fn ier(&mut self) -> IER {
        // SAFETY: We operate on valid register addresses.
        let val = unsafe { self.backend.read(offsets::IER as u8) };
        IER::from_bits_retain(val)
    }

    /// Fetches the current value from the [`ISR`].
    pub fn isr(&mut self) -> ISR {
        // SAFETY: We operate on valid register addresses.
        let val = unsafe { self.backend.read(offsets::ISR as u8) };
        ISR::from_bits_retain(val)
    }

    /// Fetches the current value from the [`LCR`].
    pub fn lcr(&mut self) -> LCR {
        // SAFETY: We operate on valid register addresses.
        let val = unsafe { self.backend.read(offsets::LCR as u8) };
        LCR::from_bits_retain(val)
    }

    /// Fetches the current value from the [`MCR`].
    pub fn mcr(&mut self) -> MCR {
        // SAFETY: We operate on valid register addresses.
        let val = unsafe { self.backend.read(offsets::MCR as u8) };
        MCR::from_bits_retain(val)
    }

    /// Fetches the current value from the [`LSR`].
    pub fn lsr(&mut self) -> LSR {
        // SAFETY: We operate on valid register addresses.
        let val = unsafe { self.backend.read(offsets::LSR as u8) };
        LSR::from_bits_retain(val)
    }

    /// Fetches the current value from the [`MSR`].
    pub fn msr(&mut self) -> MSR {
        // SAFETY: We operate on valid register addresses.
        let val = unsafe { self.backend.read(offsets::MSR as u8) };
        MSR::from_bits_retain(val)
    }

    /// Fetches the current value from the [`SPR`].
    pub fn spr(&mut self) -> SPR {
        // SAFETY: We operate on valid register addresses.
        unsafe { self.backend.read(offsets::SPR as u8) }
    }

    /// Fetches the current values from the [`DLL`] and [`DLM`].
    ///
    /// # Caution
    ///
    /// This may be critical as it will temporarily set the DLAB bit which may
    /// hinder or negatively influence normal operation.
    pub fn dll_dlm(&mut self) -> (DLL, DLM) {
        let old_lcr = self.lcr();
        // SAFETY: We operate on valid register addresses.
        unsafe {
            // Set DLAB:
            self.backend
                .write(offsets::LCR as u8, (old_lcr | LCR::DLAB).bits());

            let dll = self.backend.read(offsets::DLL as u8);
            let dlm = self.backend.read(offsets::DLM as u8);
            // Clear DLAB:
            self.backend.write(offsets::LCR as u8, old_lcr.bits());
            (dll, dlm)
        }
    }

    /// Configures the [`FCR`] from the currently active [`Config`].
    ///
    /// It will always flush all FIFO queues (RX and TX) but activate the FIFO
    /// only if it is configured to be active.
    ///
    /// # Safety
    ///
    /// This will reset the send and receive FIFOs, so data loss is possible.
    /// Call this only before setting up an actual connection.
    unsafe fn configure_fcr(&mut self) {
        // Set fifo control register.
        // SAFETY: We operate on valid register addresses.
        unsafe {
            let mut fcr = FCR::from_bits_retain(0);
            if self.config.fifo_trigger_level.is_some() {
                fcr |= FCR::FIFO_ENABLE;
            }
            fcr |= FCR::RX_FIFO_RESET;
            fcr |= FCR::TX_FIFO_RESET;
            // don't set DMA mode
            if let Some(level) = self.config.fifo_trigger_level {
                fcr = fcr.set_fifo_trigger_level(level);
            }

            self.backend.write(offsets::FCR as u8, fcr.bits());
        }
    }

    /* ----- Misc ----------------------------------------------------------- */

    /// Returns the config from the last call to [`Self::init`] together with
    /// the base address of the underlying hardware.
    ///
    /// To get the values that are currently in the registers, consider using
    /// [`Self::config_register_dump`].
    pub const fn config(&self) -> (&Config, B::Address) {
        (&self.config, self.base_address)
    }

    /// Queries the device and returns a [`ConfigRegisterDump`].
    ///
    /// # Caution
    ///
    /// Reading on some registers may have side effects, for example in the
    /// [`LSR`].
    pub fn config_register_dump(&mut self) -> ConfigRegisterDump {
        let (dll, dlm) = self.dll_dlm();
        ConfigRegisterDump {
            ier: self.ier(),
            isr: self.isr(),
            lcr: self.lcr(),
            mcr: self.mcr(),
            lsr: self.lsr(),
            msr: self.msr(),
            spr: self.spr(),
            dll,
            dlm,
        }
    }
}

// SAFETY: `Uart16550` is not `Sync`, so concurrent access from multiple
// threads is not possible through this type's API alone. Implementing `Send`
// allows moving ownership to another thread, which is safe because at any
// point only one thread holds the `&mut self` required for all operations.
// Without this, higher-level wrappers such as `Mutex<Uart16550>` could not
// be constructed, since `Mutex<T>: Sync` requires `T: Send`.
unsafe impl<B: Backend> Send for Uart16550<B> {}

/// A dump of all (readable) config registers of [`Uart16550`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ConfigRegisterDump {
    /// The current [`IER`].
    pub ier: IER,
    /// The current [`ISR`].
    pub isr: ISR,
    /// The current [`LCR`].
    pub lcr: LCR,
    /// The current [`MCR`].
    pub mcr: MCR,
    /// The current [`LSR`].
    pub lsr: LSR,
    /// The current [`MSR`].
    pub msr: MSR,
    /// The current [`SPR`].
    pub spr: SPR,
    /// The current [`DLL`].
    pub dll: DLL,
    /// The current [`DLM`].
    pub dlm: DLM,
}

impl ConfigRegisterDump {
    /// Returns the effective divisor.
    ///
    /// Using [`calc_baud_rate`], you can calculate the effective baud rate. You
    /// can also use [`Self::baud_rate()`].
    #[must_use]
    pub const fn divisor(&self) -> u16 {
        let dll = self.dll as u16;
        let dlm = self.dlm as u16;
        (dlm << 8) | dll
    }

    /// Returns the effective [`BaudRate`].
    ///
    /// Using [`calc_baud_rate`], you can calculate the effective
    /// [`BaudRate`].
    #[must_use]
    pub fn baud_rate(&self, config: &Config) -> BaudRate {
        let divisor = self.divisor();
        let baud_rate = calc_baud_rate(
            config.frequency,
            divisor as u32,
            config.prescaler_division_factor,
        )
        .expect("should be able to calculate baud rate from the given valid values");
        BaudRate::from_integer(baud_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors() {
        // SAFETY: We just test the constructor but do not access the device.
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        unsafe {
            assert2::assert!(let Ok(_) = Uart16550::new_port(0x3f8));
            assert2::assert!(let Ok(_) = Uart16550::new_port(u16::MAX - NUM_REGISTERS as u16));
            assert2::assert!(let Ok(_) = Uart16550::new_port(u16::MAX - 7));
            assert2::assert!(let Err(InvalidAddressError::InvalidBaseAddress(PortIoAddress(_))) = Uart16550::new_port(u16::MAX - 6));
            assert2::assert!(let Err(InvalidAddressError::InvalidBaseAddress(PortIoAddress(_))) = Uart16550::new_port(u16::MAX));
        }

        // SAFETY: We just test the constructor but do not access the device.
        unsafe {
            assert2::assert!(let Ok(_) = Uart16550::new_mmio(0x1000 as *mut _, 1));
            assert2::assert!(let Ok(_) = Uart16550::new_mmio(0x1000 as *mut _, 2));
            assert2::assert!(let Ok(_) = Uart16550::new_mmio(0x1000 as *mut _, 4));
            assert2::assert!(let Ok(_) = Uart16550::new_mmio(0x1000 as *mut _, 8));

            assert2::assert!(
                let Err(InvalidAddressError::InvalidStride(0)) =
                    Uart16550::new_mmio(0x1000 as *mut _, 0)
            );
            assert2::assert!(
                let Err(InvalidAddressError::InvalidStride(3)) =
                    Uart16550::new_mmio(0x1000 as *mut _, 3)
            );
            assert2::assert!(
                let Err(InvalidAddressError::InvalidStride(5)) =
                    Uart16550::new_mmio(0x1000 as *mut _, 5)
            );
            assert2::assert!(
                let Err(InvalidAddressError::InvalidStride(6)) =
                    Uart16550::new_mmio(0x1000 as *mut _, 6)
            );
            assert2::assert!(
                let Err(InvalidAddressError::InvalidStride(7)) =
                    Uart16550::new_mmio(0x1000 as *mut _, 7)
            );
            assert2::assert!(
                let Err(InvalidAddressError::InvalidStride(9)) =
                    Uart16550::new_mmio(0x1000 as *mut _, 9)
            );
        }
    }

    #[test]
    fn is_send() {
        fn accept<T: Send>() {}

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        accept::<Uart16550<PioBackend>>();
        accept::<Uart16550<MmioBackend>>();

        // TODO: add test that the type is not Sync?
    }

    #[test]
    fn mmio_dummy() {
        let config = Config {
            baud_rate: BaudRate::Baud38400,
            ..Default::default()
        };

        // stride = 1
        {
            const STRIDE: usize = 1;
            let mut memory = [0_u8; NUM_REGISTERS * STRIDE];

            // Unblock init()
            memory[offsets::LSR] = LSR::TRANSMITTER_EMPTY.bits();

            // SAFETY: We are operating on valid memory.
            let mut uart = unsafe { Uart16550::new_mmio(memory.as_mut_ptr(), STRIDE as u8) }
                .expect("should be able to create the dummy MMIO");

            uart.init(config.clone())
                .expect("should be able to initialize the dummy MMIO");

            let divisor = uart.config_register_dump().divisor();
            // DLM is same as IER and was overwritten => only look at DLM.
            let divisor = divisor & 0xff;
            assert2::check!(divisor == 3);
        }

        // stride = 4
        {
            const STRIDE: usize = 4;
            let mut memory = [0_u8; NUM_REGISTERS * STRIDE];

            // Unblock init()
            memory[offsets::LSR * STRIDE] = LSR::TRANSMITTER_EMPTY.bits();

            // SAFETY: We are operating on valid memory.
            let mut uart = unsafe { Uart16550::new_mmio(memory.as_mut_ptr(), STRIDE as u8) }
                .expect("should be able to create the dummy MMIO");

            uart.init(config)
                .expect("should be able to initialize the dummy MMIO");

            let divisor = uart.config_register_dump().divisor();
            // DLM is the same as IER and was overwritten => only look at DLM.
            let divisor = divisor & 0xff;
            assert2::check!(divisor == 3);
        }
    }
}
