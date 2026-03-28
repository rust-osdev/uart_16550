// SPDX-License-Identifier: MIT OR Apache-2.0

//! Errors that can happen when working with [`Uart16550`].

#[cfg(doc)]
use crate::Uart16550;
use crate::backend::RegisterAddress;
use crate::spec::{FIFO_SIZE, NonIntegerDivisorError};
use core::error::Error;
use core::fmt;
use core::fmt::Display;

/// The specified address is invalid because it is either null or doesn't allow
/// for <code>[NUM_REGISTERS] - 1</code> subsequent addresses.
///
/// [NUM_REGISTERS]: crate::spec::NUM_REGISTERS
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvalidAddressError<A: RegisterAddress> {
    /// The given base address is invalid, e.g., it cannot accommodate
    /// <code>[NUM_REGISTERS] - 1</code> consecutive addresses.
    ///
    /// [NUM_REGISTERS]: crate::spec::NUM_REGISTERS
    InvalidBaseAddress(A),
    /// The stride is invalid.
    ///
    /// Must be non-zero and a power of two (typically 1, 2, 4, or 8).
    /// **Only relevant for MMIO**.
    InvalidStride(u8),
}

impl<A: RegisterAddress> Display for InvalidAddressError<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaseAddress(addr) => {
                write!(f, "invalid register address: {addr:x?}")
            }
            Self::InvalidStride(stride) => {
                write!(
                    f,
                    "invalid stride {stride}: must be non-zero and a power of two (typically 1, 2, 4, or 8)"
                )
            }
        }
    }
}

impl<A: RegisterAddress> Error for InvalidAddressError<A> {}

/// The loopback test failed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LoopbackError {
    /// The device wasn't ready to send a byte.
    ///
    /// This is very unlikely as the device was previously cleared from any
    /// remaining data.
    SendError(ByteSendError),
    /// Failed to read the same byte that was just written to the device.
    UnexpectedLoopbackByte {
        /// The expected byte.
        expected: u8,
        /// The actual received byte.
        actual: u8,
    },
    /// Failed to read a whole string that was just written to the device.
    UnexpectedLoopbackMsg {
        /// The expected message (a valid UTF-8 string).
        expected: [u8; FIFO_SIZE],
        /// The actual received message.
        actual: [u8; FIFO_SIZE],
    },
}

impl Display for LoopbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SendError(e) => {
                write!(f, "loopback test failed: couldn't send data: {e}")
            }
            Self::UnexpectedLoopbackByte { expected, actual } => {
                write!(
                    f,
                    "loopback test failed: read unexpected byte! expected={expected}, actual={actual}"
                )
            }
            Self::UnexpectedLoopbackMsg { expected, actual } => {
                let expected = core::str::from_utf8(expected);
                let maybe_actual_str = core::str::from_utf8(actual);
                write!(
                    f,
                    "loopback test failed: read unexpected string! expected (str)={expected:?}, actual (str)={maybe_actual_str:?}, actual (bytes)={actual:?}"
                )
            }
        }
    }
}

impl Error for LoopbackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SendError(e) => Some(e),
            _ => None,
        }
    }
}

/// Errors that can happen when a [`Uart16550`] initialized in
/// [`Uart16550::init`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InitError {
    /// The device could not be detected.
    DeviceNotPresent,
    /// The configured baud rate can not be set as it results in an invalid
    /// divisor.
    InvalidBaudRate(NonIntegerDivisorError),
}

impl Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceNotPresent => {
                write!(f, "the device could not be detected")
            }
            Self::InvalidBaudRate(e) => {
                write!(f, "invalid baud rate: {e}")
            }
        }
    }
}

impl Error for InitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidBaudRate(err) => Some(err),
            _ => None,
        }
    }
}

/// There is currently no data to read.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ByteReceiveError;

impl Display for ByteReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "there is no data to read")
    }
}

impl Error for ByteReceiveError {}

/// Errors that happen when trying to send a byte
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ByteSendError {
    /// There is currently no capacity to send another byte.
    ///
    /// For example, the FIFO might be full.
    NoCapacity,
    /// The remote is not (yet) ready to receive more data.
    ///
    /// This can for example mean that it is still processing input data.
    RemoteNotClearToSend,
}

impl Display for ByteSendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCapacity => write!(
                f,
                "device has no capacity to send another byte (the FIFO might be full)"
            ),
            Self::RemoteNotClearToSend => {
                write!(f, "the remote didn't raised its clear to send signal")
            }
        }
    }
}

impl Error for ByteSendError {}

/// Errors indicating the device is not ready to send data.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RemoteReadyToReceiveError {
    /// There is no remote connected (DSR is not raised).
    NoRemoteConnectedNoDSR,
    /// There is no remote connected (CD is not raised).
    NoRemoteConnectedNoCD,
    /// A remote endpoint is present but has not asserted readiness to receive
    /// more data (yet).
    ///
    /// This can for example mean that it is still processing input data.
    RemoteNotClearToSend,
}

impl Display for RemoteReadyToReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRemoteConnectedNoDSR => {
                write!(
                    f,
                    "there is no remote connected: missing Data Set Ready (DSR)"
                )
            }
            Self::NoRemoteConnectedNoCD => {
                write!(
                    f,
                    "there is no remote connected: missing Carrier Detect (CD)"
                )
            }
            Self::RemoteNotClearToSend => {
                write!(f, "remote is not (yet) ready to receive more data")
            }
        }
    }
}

impl Error for RemoteReadyToReceiveError {}
