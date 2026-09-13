# uart_16550 UEFI real-hardware test

This subproject builds a UEFI application (x86_64 by default, aarch64 via
`ARCH=aarch64`) that takes ownership of 16550-compatible UARTs and exercises
this repository's driver. It is a manual integration test: automatic register
and loopback checks run first, followed by an interactive serial menu.

Diagnostics go to the UEFI console and are persisted line by line on the boot
volume as `/uart_16550_test_logs/uart_16550_<arch>_<timestamp>.txt`; a log
write failure aborts the test. Bytes written to a UART are short, recognizable
test payloads.

## TL;DR

1. Run `make artifact` (or `make artifacts` for every architecture), then
   deploy the built images to a mounted GPT/FAT32 EFI partition.
2. Boot with a monitor and USB keyboard. Leave the monitor connected: it is the
   authoritative diagnostic channel after firmware serial ownership is released.
3. Confirm the firmware baseline, configure the remote to 9600 8N1, and press
   Enter on the local keyboard.
4. Look for automatic `PASS` lines and recognizable serial payloads, then use
   the interactive commands to test the cable and reconnect behavior.

## Test scope

The application drives UARTs synchronously by polling: interrupts stay
disabled, no handler is installed, and interrupt delivery is **not** tested.
It also disables the UEFI image watchdog, since the interactive phase may take
longer than the firmware's five-minute limit; a failure to do so is a warning.

It discovers:

- COM1 at `0x3f8` unconditionally (x86_64 only);
- conventional legacy ports at `0x2f8`, `0x3e8`, and `0x2e8` when the crate's
  `check_present()` finds a device (x86_64 only);
- compatible byte-access UARTs advertised by ACPI SPCR;
- PCI serial-class controllers with an assigned, unambiguous, 16550-compatible
  BAR0, enabling its decoding when firmware left the endpoint unbound.

Unsupported ACPI interfaces and ambiguous or vendor-specific PCI layouts are
reported but not accessed. Without x86 port instructions, an I/O BAR is reached
through the PCI I/O window that the platform's ACPI DSDT declares; without an
unambiguous window the device is skipped.

## Recommended real-hardware setup

Boot the application on an x86_64 machine (see "Architecture support" for
other architectures) with:

- UEFI firmware and Secure Boot disabled, unless you sign the application;
- a monitor connected to the machine;
- a USB keyboard for Enter/Escape navigation;
- a physical serial port connected to another machine using the required
  RS-232/null-modem wiring or an appropriate USB serial cable;
- Linux and Minicom on the remote machine, for example:

```console
# If your user is part of the "dialout" group there is no need for sudo
$ sudo minicom -D /dev/ttyUSB0 -b 9600
```

Use 9600 8N1 without hardware flow control. Firmware may use a different rate
before takeover; the application prints the UEFI `SerialIo` mode and waits for
Enter before switching the UARTs to 9600 8N1.

### Remote already connected

1. Boot the USB media and watch the monitor.
2. Confirm the `UEFI SERIAL BASELINE` line also appears remotely when firmware
   serial redirection is active; its absence is valid when firmware exposes no
   serial console.
3. Set Minicom to 9600 8N1 and press Enter on the test machine's USB keyboard.
4. Confirm the `[uart_16550]` payload appears remotely.
5. Use the interactive commands below.

### Connect or reconnect during the test

Starting without the remote cable is valid too. Let the automatic tests finish,
connect the cable during the interactive phase, and then:

- type `c` to inspect DSR/CTS and modem-status changes;
- type `r` before and after reconnecting to compare registers;
- type `t` to send a known line to Minicom;
- type another printable ASCII character to test receive and echo.

Some USB serial and null-modem wiring does not expose DSR or CTS, so a
connection warning is diagnostic and does not fail otherwise working traffic.
Press Escape on the local USB keyboard or send byte `0x1b` from the remote
terminal to skip a UART that has no connected remote.

## Build

Install the Rust UEFI targets once if necessary, then build:

```console
rustup target add x86_64-unknown-uefi aarch64-unknown-uefi
make artifact
```

The result is `build/BOOTX64.EFI`; `make artifact ARCH=aarch64` produces
`build/BOOTAA64.EFI` instead, and every `make` target accepts `ARCH`.
`make artifacts` cross-compiles every supported architecture in one step.
`make check` runs all static build checks.

## Run under QEMU

The Nix development shell supplies QEMU and the firmware:

```console
nix develop
make qemu
```

The QEMU window is the UEFI monitor and keyboard; COM1 is connected to the
launching terminal. A `pci-serial` device is present as well, and QEMU prints
its `/dev/pts/...` path at startup; open it in a second terminal to exercise
PCI discovery and BAR-backed UART access:

```console
minicom -D /dev/pts/NUMBER -b 9600
```

Outside Nix, pass the firmware explicitly (`OVMF=/path/to/OVMF.fd make qemu`).
KVM is the default; `make qemu-tcg` (or `QEMU_ACCEL=tcg`) selects software
emulation. `QEMU`, `QEMU_ARGS`, and `ESP_DIR` override the executable, add
arguments, or relocate the directory-backed EFI system partition; QEMU data
stays below the ignored `target/real-hw-test/` tree.

`make qemu ARCH=aarch64` runs the aarch64 build on QEMU's `virt` machine with
the EDK2 firmware bundled with QEMU (`AAVMF_CODE`/`AAVMF_VARS` override it),
using TCG by default. The terminal shows the PL011 firmware console, which the
application correctly rejects; the only 16550 is the `pci-serial` device,
reached through the memory-mapped PCI I/O window.

## Architecture support

x86_64 is the primary target and the only one exercised on physical hardware so
far. aarch64 is fully validated under QEMU; on real aarch64 machines the test
is expected to find little today:

- Server-class Arm platforms describe a PL011 or SBSA Generic UART in SPCR,
  which is not 16550-compatible and is deliberately rejected.
- Boards whose EDK2 ports do describe a 16550 (for example RK3588) declare
  32-bit registers at stride 4; the driver only performs byte accesses, so
  such SPCR layouts are rejected as well.
- Boards booting through U-Boot's EFI implementation publish a device tree
  instead of ACPI; the test has no device-tree discovery.

riscv64 is not supported because Rust has no riscv64 UEFI target; it would
need a custom target JSON on nightly with `-Zbuild-std`. QEMU's riscv64 `virt`
machine would otherwise fit well: its ns16550a is MMIO-mapped and described by
an SPCR with the 16550 interface type.

## Reading the test output

The UEFI monitor is authoritative. Before takeover, `UEFI SERIAL BASELINE`
confirms firmware still owns the serial output; after the controllers are
disconnected, remote output may stop, so keep reading the monitor.

Each usable candidate is listed with its register access (PIO or MMIO), its
location, and the discovery paths that found it. Locations are `built-in
legacy port`, `built-in platform UART` (memory-mapped, described by firmware
without a PCI identity), or a PCI function with vendor/device IDs, `on the
root bus` (typically integrated) or `behind a bridge` (typically an add-in
card); known QEMU devices are named. Discovery paths are `required COM1`,
`presence check at a conventional port`, `ACPI SPCR`, and `PCI enumeration`.

Good signs are:

- `PASS` for crate `init`, initialized register values, crate loopback, and
  send APIs;
- `[uart_16550]` lines on the remote terminal;
- `PASS: interactive loopback`, echoed printable characters, and transmitted
  `[interactive]` lines during manual testing;
- a final summary with each required UART marked `PASS`.

`WARN: connection signals` or a DSR/CTS warning is expected with a three-wire
or USB serial cable that provides no modem-control lines; an Escape skip is a
warning as well. Investigate `FAIL`, `SKIP`, transmit or receive timeouts, a
failed `disconnect_controller`, or a register/loopback mismatch, starting with
the candidate address, its reported clock, the 9600 8N1 settings, cable
crossover and ground, and the remote terminal.

## Interactive commands

Commands are read from the UART currently named on the monitor:

| Input                         | Expected result                                           |
| ----------------------------- | --------------------------------------------------------- |
| `r`                           | Register snapshot appears on the UEFI screen.             |
| `t`                           | `[interactive]` test line appears on the remote terminal. |
| `c`                           | Screen shows DSR/CTS status and a fresh register dump.    |
| `l`                           | Screen reports `PASS: interactive loopback` or a failure. |
| `q`                           | This UART completes and the next candidate begins.        |
| Printable ASCII               | Screen shows the byte and the remote receives its echo.   |
| Local Escape or serial `0x1b` | Skip this UART with a `WARN` diagnostic.                  |

The final screen reports `PASS` (required automatic checks succeeded), `WARN`
(they succeeded, but connection signals were absent or the interactive phase
was skipped), or `FAIL` (presence, initialization, register, loopback, or
transmit readiness failed).

## Troubleshooting

- No firmware baseline remotely: firmware may not expose or use `SerialIo`.
  COM1 is still probed and tested after takeover.
- Garbled characters: confirm both ends use 9600 8N1 after the Enter prompt.
- No traffic: verify TX/RX crossover, common ground, RS-232 voltage conversion,
  and whether a null-modem adapter is required.
- DSR/CTS warning with working bytes: the cable likely omits modem-control
  lines; leave hardware flow control disabled.
- PCI controller is skipped: its programming interface, BAR, decoding state,
  or layout was not safe to treat as a standard 16550 endpoint.
- QEMU does not start with KVM: use `make qemu-tcg`.
