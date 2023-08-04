# uart_16550

[![Build Status](https://github.com/rust-osdev/uart_16550/workflows/Build/badge.svg)](https://github.com/rust-osdev/uart_16550/actions?query=workflow%3ABuild) [![Docs.rs Badge](https://docs.rs/uart_16550/badge.svg)](https://docs.rs/uart_16550/)

Minimal support for [serial communication](https://en.wikipedia.org/wiki/Asynchronous_serial_communication) through [UART](https://en.wikipedia.org/wiki/Universal_asynchronous_receiver-transmitter) devices, which are compatible to the [16550 UART](https://en.wikipedia.org/wiki/16550_UART). This crate supports I/O port-mapped (x86 only) and memory-mapped UARTS.

## Usage

Depending on the system architecture, the UART can be either accessed through [port-mapped I/O](https://wiki.osdev.org/Port_IO) or [memory-mapped I/O](https://en.wikipedia.org/wiki/Memory-mapped_I/O).

### With port-mappd I/O

The UART is accessed through port-mapped I/O on architectures such as `x86_64`. On these architectures, the [`SerialPort`](https://docs.rs/uart_16550/~0.2/uart_16550/struct.SerialPort.html) type can be used:


```rust
use uart_16550::SerialPort;

const SERIAL_IO_PORT: u16 = 0x3F8;

let mut serial_port = unsafe { SerialPort::new(SERIAL_IO_PORT) };
serial_port.init();

// Now the serial port is ready to be used. To send a byte:
serial_port.send(42);

// To receive a byte:
let data = serial_port.receive();
```

### With memory mapped serial port

Most other architectures, such as [RISC-V](https://en.wikipedia.org/wiki/RISC-V), use memory-mapped I/O for accessing the UARTs. On these architectures, the [`MmioSerialPort`](https://docs.rs/uart_16550/~0.2/uart_16550/struct.MmioSerialPort.html) type can be used:

```rust
use uart_16550::MmioSerialPort;

const SERIAL_PORT_BASE_ADDRESS: usize = 0x1000_0000;

let mut serial_port = unsafe { MmioSerialPort::new(SERIAL_PORT_BASE_ADDRESS) };
serial_port.init();

// Now the serial port is ready to be used. To send a byte:
serial_port.send(42);

// To receive a byte:
let data = serial_port.receive();
```

## Building with stable rust

This needs to have the [compile-time requirements](https://github.com/alexcrichton/cc-rs#compile-time-requirements) of the `cc` crate installed on your system.
It was currently only tested on Linux and MacOS.

## License

Licensed under the MIT license ([LICENSE](LICENSE) or <http://opensource.org/licenses/MIT>).
