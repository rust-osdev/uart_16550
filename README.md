# uart_16550

Simple yet highly configurable low-level driver for
[16550 UART devices][uart], typically known and used as serial ports or
COM ports. Easy integration into Rust while providing fine-grained control
where needed (e.g., for kernel drivers).

The "serial device" or "COM port" in typical x86 machines is almost always
backed by a **16550 UART devices**, may it be physical or emulated. This
crate offers convenient and powerful abstractions for these devices, and
also works for other architectures, such as ARM or RISC-V, by offering
support for MMIO-mapped devices.

Serial ports are especially useful for debugging or operating system
learning projects. See [`Uart16550`] to get started.

[`Uart16550`]: https://docs.rs/uart_16550/latest/uart_16550/struct.Uart16550.html

## Features

- ✅ Full configure, transmit, receive, and interrupt support for UART
  16550–compatible devices
- ✅ High-level, ergonomic abstractions and types paired with support for
  plain integers
- ✅ Straightforward to integrate, highly configurable when needed
- ✅ Validated on **real hardware** as well as across different virtual
  machines
- ✅ Fully type-safe and derived directly from the official
  [specification][uart]
- ✅ Supports both **x86 port-mapped I/O** and **memory-mapped I/O** (MMIO)
- ✅ `no_std`-compatible and allocation-free by design
- ✅ Compatible with all architectures supported by Rust (x86/x86_64, ARM,
    RISC-V, ...)

## Focus, Scope & Limitations

While serial ports are often used in conjunction with VT102-like terminal
emulation, the primary focus of this crate is strict specification
compliance and convenient direct access to the underlying hardware for
transmitting and receiving bytes, including all necessary device
configuration.

For basic terminal-related functionality, such as newline normalization and
backspace handling, we provide `Uart16550Tty` as a **basic** convenience
layer.

# Overview

Use `Uart16550Tty` for a quick start to write to a terminal via your serial
connection. For more fine-grained low-level control, please have a look at
`Uart16550` instead.

# Example (Minimal - x86 Port IO)

```rust
use uart_16550::{Config, Uart16550Tty};
use core::fmt::Write;

fn main() {
  // SAFETY: The port is valid and we have exclusive access.
  let mut uart = unsafe { Uart16550Tty::new_port(0x3f8, Config::default()).expect("should initialize device") };
  uart.write_str("hello world\nhow's it going?");
}
```

# Example (Minimal - MMIO)

```rust
use uart_16550::{Config, Uart16550Tty};
use core::fmt::Write;

fn main() {
  // SAFETY: The address is valid and we have exclusive access.
  let mut uart = unsafe { Uart16550Tty::new_mmio(0x1000 as *mut _, 4, Config::default()).expect("should initialize device") };
  uart.write_str("hello world\nhow's it going?");
}
```

# Example (More low-level control)

```rust
use uart_16550::{Config, Uart16550};

fn main() {
  // SAFETY: The address is valid and we have exclusive access.
  let mut uart = unsafe { Uart16550::new_mmio(0x1000 as *mut _, 4).expect("should be valid port") };
  //                                 ^ or `new_port(0x3f8)`
  uart.init(Config::default()).expect("should init device successfully");
  uart.test_loopback().expect("should have working loopback mode");
  // Note: Might fail on real hardware with some null-modem cables
  uart.check_connected().expect("should have physically connected receiver");
  uart.send_bytes_exact(b"hello world!");
}
```

## License

This project is licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))


## Changelog

See [CHANGELOG.md](./CHANGELOG.md).

[uart]: https://en.wikipedia.org/wiki/16550_UART
