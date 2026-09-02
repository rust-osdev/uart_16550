# uart_16550 UEFI real-hardware test

This subproject builds a UEFI application (x86_64 by default, aarch64 via
`ARCH=aarch64`) that takes ownership of 16550-compatible UARTs and exercises
this repository's driver. It is a manual integration test: automatic register
and loopback checks run first, followed by an interactive serial menu.

All diagnostics use UEFI Simple Text Output and are intended to stay visible on
the test machine's monitor. They are also persisted, line by line, on the boot
volume as `/uart_16550_test_logs/uart_16550_<arch>_YYYY-MM-DD_HH-MM-SS.txt`. A log
creation, write, or flush failure is critical and aborts the test. Bytes
written directly to a UART are deliberately short, recognizable test payloads.

## TL;DR

1. Run `make artifact` (or `make artifacts` for every architecture), then
   deploy the built images with `make install` to a mounted GPT/FAT32 EFI
   partition.
2. Boot with a monitor and USB keyboard. Leave the monitor connected: it is the
   authoritative diagnostic channel after firmware serial ownership is released.
3. Confirm the firmware baseline, configure the remote to 9600 8N1, and press
   Enter on the local keyboard.
4. Look for automatic `PASS` lines and recognizable serial payloads, then use
   the interactive commands to test the cable and reconnect behavior.

## Test scope

The application runs UARTs synchronously using polling. It disables UART
interrupts, installs no interrupt handler, and does **not** validate interrupt
delivery or interrupt-driven transmit/receive behavior.

It also disables the UEFI image watchdog because an interactive serial phase
may legitimately take longer than the firmware's normal five-minute limit. A
failure to disable it is reported as a warning on screen.

It discovers:

- COM1 at `0x3f8` unconditionally (x86_64 only);
- conventional legacy ports at `0x2f8`, `0x3e8`, and `0x2e8` when their scratch
  registers behave like a UART (x86_64 only);
- compatible byte-access UARTs advertised by ACPI SPCR;
- PCI serial-class controllers with an assigned, unambiguous, 16550-compatible
  BAR0, enabling its decoding when firmware left the endpoint unbound.

Unsupported ACPI interfaces and ambiguous or vendor-specific PCI layouts are
reported but not accessed. Without x86 port instructions, an I/O BAR is reached
through the memory-mapped PCI I/O window that the platform's ACPI DSDT
declares; a missing or ambiguous window skips the device.

## Recommended real-hardware setup

Boot the application on an x86_64 machine (see "Architecture support" for the
state of other architectures) with:

- UEFI firmware and Secure Boot disabled, unless you sign the application;
- a monitor connected to the machine;
- a USB keyboard for Enter/Escape navigation;
- a physical serial port connected to another machine using the required
  RS-232/null-modem wiring or an appropriate USB serial cable;
- Linux and Minicom on the remote machine.

For example, adjust the remote device name as needed:

```console
minicom -D /dev/ttyUSB0 -b 9600
```

Use 9600 baud, 8 data bits, no parity, one stop bit, and no hardware flow
control. Firmware may use a different rate before takeover. The application
prints the UEFI `SerialIo` mode and waits for Enter before switching the UARTs
to 9600 8N1.

### Remote already connected

1. Boot the USB media and watch the monitor.
2. Confirm the `UEFI SERIAL BASELINE` line also appears remotely when firmware
   serial redirection is active. Absence is valid when firmware exposes no
   serial console.
3. Set Minicom to 9600 8N1 and press Enter on the test machine's USB keyboard.
4. Confirm `[barebones]` and `[uart_16550]` payloads appear remotely.
5. Use the interactive commands below.

### Connect or reconnect during the test

It is also valid to start without the remote cable connected. Let the automatic
tests finish, connect the cable during the interactive phase, and then:

- type `c` to inspect DSR/CTS and modem-status changes;
- type `r` before and after reconnecting to compare registers;
- type `t` to send a known line to Minicom;
- type another printable ASCII character to test receive and echo.

Some USB serial and null-modem wiring does not expose DSR or CTS. A connection
warning is therefore diagnostic and does not fail otherwise working traffic.
Press Escape on the local USB keyboard or send byte `0x1b` from the remote
terminal to skip a UART that has no connected remote.

## Build

Install the Rust UEFI targets once if necessary:

```console
rustup target add x86_64-unknown-uefi aarch64-unknown-uefi
```

Then build and stage the removable-media filename:

```console
make artifact
file build/BOOTX64.EFI
```

The resulting file is `build/BOOTX64.EFI`. `make artifact ARCH=aarch64`
produces `build/BOOTAA64.EFI` instead; every `make` target accepts the same
`ARCH` variable. `make artifacts` cross-compiles every supported architecture
in one step.

Run all static build checks with:

```console
make check
```

## Install on USB media

Prepare and mount an EFI partition yourself. The install target intentionally
does not partition, format, mount, or unmount devices. It verifies that the
mount is backed by a partition on a GPT disk and that `lsblk` identifies the
filesystem as FAT32 before copying anything.

Inspect the target carefully:

```console
lsblk -o NAME,SIZE,TYPE,FSTYPE,FSVER,PTTYPE,MOUNTPOINTS
make install USB_MOUNT=/run/media/$USER/EFI
```

Every artifact present in `build/` is copied to its removable-media path, so
one stick can boot every architecture built beforehand (for example with
`make artifacts`): `EFI/BOOT/BOOTX64.EFI`, `EFI/BOOT/BOOTAA64.EFI`, and so on.
If the disk is not GPT, the
filesystem is not FAT32, the path is not an exact mount point, or the mount is
not writable, installation stops with a diagnostic. Unmount the partition
cleanly before removing it.

## Run under QEMU

The included Nix development shell supplies QEMU and OVMF:

```console
nix develop
make qemu
```

The graphical QEMU window is the UEFI monitor and keyboard. COM1 is connected
to the terminal that launched QEMU. A `pci-serial` device is also present; QEMU
prints its `/dev/pts/...` path during startup. Open that PTY in a second terminal
to exercise PCI discovery and BAR-backed UART access:

```console
minicom -D /dev/pts/NUMBER -b 9600
```

Outside Nix, provide the combined OVMF image explicitly:

```console
OVMF=/path/to/OVMF.fd make qemu
```

KVM is used by default. Use software emulation when KVM is unavailable:

```console
make qemu-tcg
# equivalent: QEMU_ACCEL=tcg make qemu
```

`QEMU`, `QEMU_ARGS`, and `ESP_DIR` can override the executable, add QEMU
arguments, or relocate the temporary directory-backed EFI system partition.
QEMU data stays below the repository's ignored `target/real-hw-test/` tree.

`make qemu ARCH=aarch64` runs the aarch64 build on QEMU's `virt` machine with
the EDK2 firmware bundled with QEMU (override via `AAVMF_CODE`/`AAVMF_VARS`).
The terminal shows the PL011 firmware console, which the application correctly
rejects as a UART candidate; the only 16550 is the `pci-serial` device, reached
through the memory-mapped PCI I/O window. TCG is the default accelerator for
the aarch64 guest.

### Headless CI smoke test

`make ci-qemu` boots the same artifact as `make artifact` headlessly with QEMU
TCG; the application contains no CI-specific code. A host-side script answers
the operator prompts and skips the interactive phase through QEMU-monitor
`sendkey`, then judges the run by the log the application persists on its boot
volume and by the serial captures. On x86_64 it requires both legacy COM1 and
the QEMU PCI serial controller to be discovered; on aarch64 it requires the
PL011 console to be rejected and the PCI UART to be driven through the
translated I/O window. Every deterministic raw and `uart_16550` check must
pass.

The harness needs `socat`, `mtools`, and `dosfstools` next to QEMU and the
firmware; the Nix development shell provides all of them.

```console
make ci-qemu
make ci-qemu ARCH=aarch64
```

This smoke test is useful for debugging the test application and preventing its
automatic QEMU paths from regressing. It does not replace the manual test of a
real cable, reconnect behavior, firmware-specific ownership handoff, or
physical hardware.

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

riscv64 is currently not supported because Rust has no riscv64 UEFI target;
building would require a custom target JSON on nightly with `-Zbuild-std`.
QEMU's riscv64 `virt` machine would otherwise be a good fit: its ns16550a is
MMIO-mapped and described by an SPCR with the 16550 interface type.

## Reading the test output

The UEFI monitor is authoritative. Before takeover, `UEFI SERIAL BASELINE`
confirms firmware still owns the serial output. After controllers are
disconnected, remote output may stop; continue reading diagnostics on the
monitor.

Good signs are:

- `PASS` for raw initialization, loopback, register invariants, crate `init`,
  crate loopback, and send APIs;
- `[barebones]` and `[uart_16550]` lines on the remote terminal;
- `PASS: interactive loopback`, echoed printable characters, and transmitted
  `[interactive]` lines during manual testing;
- a final summary with each required UART marked `PASS`.

`WARN: connection signals` or a DSR/CTS warning can be expected with a
three-wire or USB serial cable that does not provide modem-control lines. A
local or serial Escape skip is also a warning, not an automatic test failure.

Investigate `FAIL`, `SKIP`, transmit or receive timeouts, a failed
`disconnect_controller`, an initialization/register/loopback mismatch, or a
final summary containing `FAIL`. Start with the candidate address, its reported
clock, 9600 8N1 settings, cable crossover and ground, and the remote terminal.

## Interactive commands

Commands are read from the UART currently named on the monitor:

| Input | Expected result |
| --- | --- |
| `r` | Register snapshot appears on the UEFI screen. |
| `t` | `[interactive]` test line appears on the remote terminal. |
| `c` | Screen shows DSR/CTS status and a fresh register dump. |
| `l` | Screen reports `PASS: interactive loopback` or a failure. |
| `q` | This UART completes and the next candidate begins. |
| Printable ASCII | Screen shows the byte and the remote receives its echo. |
| Local Escape or serial `0x1b` | Skip this UART with a `WARN` diagnostic. |

The final screen reports:

- `PASS`: required automatic checks succeeded;
- `WARN`: automatic checks succeeded but connection signals were absent or the
  interactive phase was skipped;
- `FAIL`: presence, initialization, register, loopback, or transmit readiness
  failed.

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
