#!/usr/bin/env bash
set -euo pipefail

arch=${ARCH:-x86_64}
qemu=${QEMU:-qemu-system-$arch}
ovmf=${OVMF:-}
artifact=${1:-build/BOOTX64.EFI}
run_dir=${CI_RUN_DIR:-../target/real-hw-test/qemu-ci-$arch}
timeout_s=${CI_TIMEOUT:-300}

boot_name=$(basename "$artifact")
disk=$run_dir/disk.img
monitor=$run_dir/monitor.sock
console_log=$run_dir/console.log
pci_log=$run_dir/pci-serial.log
persisted_log=$run_dir/persisted.log

for tool in "$qemu" socat truncate mkfs.vfat mmd mcopy; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: required tool not found: $tool" >&2
        exit 2
    fi
done
if [[ ! -r "$artifact" ]]; then
    echo "error: UEFI artifact is missing: $artifact (run 'make artifact')" >&2
    exit 2
fi
if [[ -z "$run_dir" || "$run_dir" == "/" ]]; then
    echo "error: refusing unsafe CI_RUN_DIR: $run_dir" >&2
    exit 2
fi

# Per-architecture machine, firmware, and input configuration. Both guests get
# a PCI 16550; only q35 additionally has legacy COM1 and a built-in keyboard.
case "$arch" in
x86_64)
    if [[ -z "$ovmf" ]]; then
        echo "error: OVMF is unset; set OVMF=/path/to/OVMF.fd" >&2
        exit 2
    fi
    if [[ ! -r "$ovmf" ]]; then
        echo "error: OVMF firmware is not readable: $ovmf" >&2
        exit 2
    fi
    machine_args=(-machine q35,accel=tcg -m 256M -bios "$ovmf")
    ;;
aarch64)
    # QEMU ships pflash-style EDK2 images next to its own installation.
    share_dir=$(dirname "$(readlink -f "$(command -v "$qemu")")")/../share/qemu
    aavmf_code=${AAVMF_CODE:-$share_dir/edk2-aarch64-code.fd}
    aavmf_vars=${AAVMF_VARS:-$share_dir/edk2-arm-vars.fd}
    for firmware in "$aavmf_code" "$aavmf_vars"; do
        if [[ ! -r "$firmware" ]]; then
            echo "error: aarch64 firmware is not readable: $firmware" >&2
            echo "       set AAVMF_CODE and AAVMF_VARS" >&2
            exit 2
        fi
    done
    # The UEFI keyboard prompts need an input device; virt has none built in.
    machine_args=(-machine virt,accel=tcg -cpu max -m 512M
        -drive "if=pflash,format=raw,file=$aavmf_code,readonly=on"
        -drive "if=pflash,format=raw,file=$run_dir/vars.fd"
        -device qemu-xhci -device usb-kbd)
    ;;
*)
    echo "error: unsupported ARCH '$arch'; supported: x86_64, aarch64" >&2
    exit 2
    ;;
esac

# A fresh boot image and logs ensure the result cannot come from a previous run.
rm -rf "$run_dir"
mkdir -p "$run_dir"
if [[ "$arch" == aarch64 ]]; then
    # Writable per-run variable store; the template may be read-only on disk.
    cp "$aavmf_vars" "$run_dir/vars.fd"
    chmod u+w "$run_dir/vars.fd"
fi

# A real FAT image instead of QEMU's experimental fat:rw: directory makes the
# log that the application persists on its boot volume readable on the host.
truncate -s 64M "$disk"
mkfs.vfat "$disk" >/dev/null
mmd -i "$disk" ::/EFI ::/EFI/BOOT
mcopy -i "$disk" "$artifact" "::/EFI/BOOT/$boot_name"

"$qemu" \
    "${machine_args[@]}" \
    -drive "format=raw,file=$disk" \
    -nic none \
    -display none \
    -monitor "unix:$monitor,server,nowait" \
    -serial "file:$console_log" \
    -chardev "file,id=pci_serial,path=$pci_log" \
    -device pci-serial,chardev=pci_serial \
    -no-reboot &
qemu_pid=$!
trap 'kill -9 "$qemu_pid" 2>/dev/null || true' EXIT
deadline=$(($(date +%s) + timeout_s))

monitor_cmd() {
    printf '%s\n' "$1" | socat -t 1 - "UNIX-CONNECT:$monitor" >/dev/null 2>&1 \
        || true
}

qemu_alive() {
    kill -0 "$qemu_pid" 2>/dev/null
}

# Mid-run extraction from the live FAT image is best-effort progress polling;
# only the extraction after QEMU quit is authoritative.
extract_persisted_log() {
    rm -rf "$run_dir/logs"
    mkdir -p "$run_dir/logs"
    MTOOLS_SKIP_CHECK=1 mcopy -n -s -i "$disk" ::/uart_16550_test_logs \
        "$run_dir/logs/" >/dev/null 2>&1 || true
    cat "$run_dir/logs/uart_16550_test_logs"/*.txt 2>/dev/null || true
}

dump_logs() {
    if [[ -s "$persisted_log" ]]; then
        echo "--- $persisted_log ---" >&2
        cat "$persisted_log" >&2
    fi
    for log in "$console_log" "$pci_log"; do
        if [[ -s "$log" ]]; then
            echo "--- $log ---" >&2
            sed -n '1,200p' "$log" >&2
        fi
    done
}

fail_run() {
    extract_persisted_log > "$persisted_log"
    dump_logs
    echo "FAIL: $1" >&2
    exit 1
}

# The application persists every line before displaying it, so the extracted
# log doubles as the progress signal. No key is sent before the first operator
# prompt: an Escape while firmware still owns the keyboard would enter the
# firmware setup menu instead of the boot target.
until extract_persisted_log | grep -qF 'then press Enter.'; do
    qemu_alive || fail_run "QEMU exited before the first operator prompt"
    if (($(date +%s) >= deadline)); then
        fail_run "timeout waiting for the first operator prompt"
    fi
    sleep 2
done

# Enter satisfies both operator confirmations, which discard every other key,
# and Escape skips each per-UART interactive phase, which ignores Enter.
# Blindly alternating both keys drives the application to its final prompt.
while (($(date +%s) < deadline)); do
    if extract_persisted_log | grep -qF 'Press Enter to return to firmware.'; then
        break
    fi
    # A queued Enter can finish the final prompt early; the persisted log then
    # already contains every line the assertions below need.
    qemu_alive || break
    monitor_cmd 'sendkey ret'
    sleep 1
    monitor_cmd 'sendkey esc'
    sleep 1
done

# A monitor quit lets QEMU commit the final FAT state before extraction.
monitor_cmd quit
for _ in $(seq 15); do
    qemu_alive || break
    sleep 1
done
kill -9 "$qemu_pid" 2>/dev/null || true
wait "$qemu_pid" 2>/dev/null || true

extract_persisted_log > "$persisted_log"
if ! grep -qF 'Press Enter to return to firmware.' "$persisted_log"; then
    dump_logs
    echo "FAIL: the application did not reach its final prompt in ${timeout_s}s" >&2
    exit 1
fi

failures=0
assert_log() {
    local file=$1
    shift
    if ! grep -q "$@" "$file"; then
        echo "FAIL: expected $file to match: $*" >&2
        failures=$((failures + 1))
    fi
}

# The per-architecture UART topology every run must fully discover and pass.
case "$arch" in
x86_64)
    # Legacy COM1 plus the PCI UART, both driven through their captures.
    assert_log "$persisted_log" -F 'PIO 0x03f8'
    assert_log "$persisted_log" -F 'RequiredCom1'
    assert_log "$persisted_log" -E 'sources=\[.*Pci'
    assert_log "$persisted_log" -F 'Final summary: 2/2 passed'
    assert_log "$persisted_log" -F '2 interactive skip(s)'
    assert_log "$console_log" -F '[barebones] uart transmit test'
    assert_log "$console_log" -F '[uart_16550] uart transmit test'
    assert_log "$pci_log" -F '[barebones] uart transmit test'
    assert_log "$pci_log" -F '[uart_16550] uart transmit test'
    ;;
aarch64)
    # The PL011 console must be rejected; the PCI UART is reached through the
    # ACPI-described I/O window and driven via the MMIO backend.
    assert_log "$persisted_log" -F 'SKIP: SPCR interface is not 16450/16550-compatible'
    assert_log "$persisted_log" -F 'I/O window translation:'
    assert_log "$persisted_log" -E 'sources=\[.*Pci'
    assert_log "$persisted_log" -F 'Final summary: 1/1 passed'
    assert_log "$persisted_log" -F '1 interactive skip(s)'
    assert_log "$pci_log" -F '[barebones] uart transmit test'
    assert_log "$pci_log" -F '[uart_16550] uart transmit test'
    ;;
esac

if ((failures > 0)); then
    dump_logs
    exit 1
fi

echo "PASS: headless $arch TCG run drove the UART checks to completion"
