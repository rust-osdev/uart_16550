# Integration Tests

This directory contains architecture-specific integration tests for
`uart_16550`.

## Layout

- `x86/`: boots a tiny x86 guest and verifies 16550 access through
  port I/O.
- `riscv/`: boots a tiny RISC-V guest on QEMU `virt` and verifies
  16550 access through MMIO.

## Usage

- `make`: build all integration test guests.
- `make run`: run all integration tests.
- `make run_x86`: run the x86 tests only.
- `make run_riscv`: run the RISC-V MMIO test only.
- `make clean`: clean both test builds.

## Notes

- The x86 test covers QEMU directly and tries Cloud Hypervisor when
  the host supports it.
- The RISC-V test uses QEMU's `virt` machine because it exposes an
  MMIO `ns16550a` UART at `0x1000_0000`.
