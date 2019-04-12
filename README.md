# uart_16550

[![Azure DevOps builds](https://img.shields.io/azure-devops/build/rust-osdev/uart_16550/4.svg?style=flat-square)](https://dev.azure.com/rust-osdev/uart_16550/_build?definitionId=4)

Minimal support for uart_16550 serial output.

## Usage

```rust
use uart_16550::SerialPort;

const SERIAL_IO_PORT: u16 = 0x3F8;

let mut serial_port = unsafe { SerialPort::new(SERIAL_IO_PORT) };
serial_port.init();

// Now the serial port is ready to be used. To send a byte:
serial_port.send(42);
```

## License

Licensed under the MIT license ([LICENSE](LICENSE) or <http://opensource.org/licenses/MIT>).
