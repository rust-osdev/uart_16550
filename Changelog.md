# Unreleased

# 0.3.2 – 2024-11-13

- Add `MmioSerialPort::new_with_stride` function ([#36](https://github.com/rust-osdev/uart_16550/pull/36))

# 0.3.1 – 2024-07-11

- Add `try_send_raw` and `try_receive` ([#34](https://github.com/rust-osdev/uart_16550/pull/34))
- Update bitflags dependency to version 2 ([#33](https://github.com/rust-osdev/uart_16550/pull/33))

# 0.3.0 – 2023-08-04

- Internal rewrite of port operations to work on both `x86` and `x86_64` ([#29](https://github.com/rust-osdev/uart_16550/pull/29))

# 0.2.19 – 2023-07-07

- Make crate usable for 32-bit `x86` ([#28](https://github.com/rust-osdev/uart_16550/pull/28))

# 0.2.18 – 2022-04-16

- Remove use of `stable` and `nightly` features ([#24](https://github.com/rust-osdev/uart_16550/pull/24))

# 0.2.17 – 2022-03-28

- Remove stabilized nightly feature 'const_ptr_offset' ([#22](https://github.com/rust-osdev/uart_16550/pull/22))

# 0.2.16 – 2022-01-08

- Add `send_raw()` function to allow sending arbitrary binary data using the serial port ([#21](https://github.com/rust-osdev/uart_16550/pull/21))

# 0.2.15 – 2021-06-06

- Add support for memory mapped UARTs ([#15](https://github.com/rust-osdev/uart_16550/pull/15))
- Improvements to new MMIO support ([#18](https://github.com/rust-osdev/uart_16550/pull/18))

# 0.2.14 – 2021-05-14

- `SerialPort::new()` no longer requires `nightly` feature ([#16](https://github.com/rust-osdev/uart_16550/pull/16))

# 0.2.13 – 2021-04-30

- Update x86_64 dependency and make it more robust ([#14](https://github.com/rust-osdev/uart_16550/pull/14))

# 0.2.12 – 2021-02-02

- Fix build on nightly by updating to x86_64 v0.13.2 ([#12](https://github.com/rust-osdev/uart_16550/pull/12))

# 0.2.11 – 2021-01-15

- Use stabilized `hint::spin_loop` instead of deprecated `atomic::spin_loop_hint`

# 0.2.10 – 2020-10-01

- Fix default feature breakage ([#11](https://github.com/rust-osdev/uart_16550/pull/11))

# 0.2.9 – 2020-09-29

- Update `x86_64` dependency to version `0.12.2`

# 0.2.8 – 2020-09-24

- Update `x86_64` dependency to version `0.12.1`

# 0.2.7

- Update `x86_64` dependency to version `0.11.0`

# 0.2.6

- Use `spin_loop_hint` while waiting for data ([#9](https://github.com/rust-osdev/uart_16550/pull/9))
- Update `x86_64` dependency to version `0.10.2`

# 0.2.5

- Support receiving bytes from serial ports ([#8](https://github.com/rust-osdev/uart_16550/pull/8))

# 0.2.4

- Enable usage with non-nightly rust ([#7](https://github.com/rust-osdev/uart_16550/pull/7))

# 0.2.3

- Cargo.toml: update x86_64 dependency ([#5](https://github.com/rust-osdev/uart_16550/pull/5))
- Switch CI to GitHub Actions ([#6](https://github.com/rust-osdev/uart_16550/pull/6))

# 0.2.2

- Update internal x86_64 dependency to version 0.8.3 ([#4](https://github.com/rust-osdev/uart_16550/pull/4))

# 0.2.1

- Update to x86_64 0.7.3 and bitflags 1.1.0 ([#1](https://github.com/rust-osdev/uart_16550/pull/1))
- Document how serial port is configured by default ([#2](https://github.com/rust-osdev/uart_16550/pull/1))
