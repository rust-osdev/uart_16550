#!/usr/bin/env bash
set -euo pipefail

qemu=${QEMU:-qemu-system-x86_64}
accel=${QEMU_ACCEL:-kvm}
ovmf=${OVMF:-}
esp_dir=${ESP_DIR:-../target/real-hw-test/qemu-esp}
artifact=${ARTIFACT:-build/BOOTX64.EFI}

if [[ -z "$ovmf" ]]; then
    echo "error: OVMF is unset; run 'nix develop' or set OVMF=/path/to/OVMF.fd" \
        >&2
    exit 2
fi
if [[ ! -r "$ovmf" ]]; then
    echo "error: OVMF firmware is not readable: $ovmf" >&2
    exit 2
fi
if ! command -v "$qemu" >/dev/null 2>&1; then
    echo "error: QEMU executable not found: $qemu" >&2
    exit 2
fi
if [[ ! -r "$artifact" ]]; then
    echo "error: UEFI artifact is missing: $artifact (run 'make artifact')" >&2
    exit 2
fi

# Recreate the virtual ESP so QEMU never boots a stale application.
rm -rf "$esp_dir"
mkdir -p "$esp_dir/EFI/BOOT"
cp "$artifact" "$esp_dir/EFI/BOOT/BOOTX64.EFI"

echo "QEMU COM1 is attached to this terminal."
echo "QEMU will print a /dev/pts/... path for the PCI serial device."

exec "$qemu" \
    -machine "q35,accel=$accel" \
    -m 256M \
    -bios "$ovmf" \
    -drive "format=raw,file=fat:rw:$esp_dir" \
    -nic none \
    -monitor none \
    -serial stdio \
    -chardev pty,id=pci_serial \
    -device pci-serial,chardev=pci_serial \
    "$@"
