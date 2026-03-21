//! API glue for [`Uart16550`] with [`embedded_io`].

use core::convert::Infallible;
use core::hint;

use embedded_io::{ErrorType, Read, ReadReady, Write, WriteReady};

use crate::Uart16550;
use crate::backend::Backend;

impl<B: Backend> ErrorType for Uart16550<B> {
    type Error = Infallible;
}

impl<B: Backend> Write for Uart16550<B> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        loop {
            let n = self.send_bytes(buf);
            if n > 0 {
                return Ok(n);
            }
            hint::spin_loop();
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<B: Backend> WriteReady for Uart16550<B> {
    fn write_ready(&mut self) -> Result<bool, Self::Error> {
        let write_ready = self.ready_to_send().is_ok();

        Ok(write_ready)
    }
}

impl<B: Backend> Read for Uart16550<B> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let n = self.receive_bytes(buf);
            if n > 0 {
                return Ok(n);
            }
            hint::spin_loop();
        }
    }
}

impl<B: Backend> ReadReady for Uart16550<B> {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        let read_ready = self.ready_to_receive().is_ok();

        Ok(read_ready)
    }
}
