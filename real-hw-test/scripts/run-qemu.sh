#!/usr/bin/env bash
set -euo pipefail

arch=${ARCH:-x86_64}
qemu=${QEMU:-qemu-system-$arch}
accel=${QEMU_ACCEL:-kvm}
esp_dir=${ESP_DIR:-target/qemu-esp-$arch}
artifact=${ARTIFACT:-}

if ! command -v "$qemu" >/dev/null 2>&1; then
    echo "error: QEMU executable not found: $qemu" >&2
    exit 2
fi

# Recreate the virtual ESP so QEMU never boots a stale application.
rm -rf "$esp_dir"
mkdir -p "$esp_dir/EFI/BOOT"

# shellcheck source=scripts/qemu-machine.sh
. "$(dirname "$0")/qemu-machine.sh"
qemu_machine_setup "$arch" "$qemu" "$accel" "$esp_dir-vars.fd"
if [[ "$arch" == aarch64 ]]; then
    # The operator watches the UEFI monitor; virt has no built-in display.
    machine_args+=(-device ramfb)
    artifact=${artifact:-build/BOOTAA64.EFI}
else
    artifact=${artifact:-build/BOOTX64.EFI}
fi

if [[ ! -r "$artifact" ]]; then
    echo "error: UEFI artifact is missing: $artifact (run 'make artifact')" >&2
    exit 2
fi
cp "$artifact" "$esp_dir/EFI/BOOT/$(basename "$artifact")"

echo "QEMU serial console is attached to this terminal."
echo "QEMU will print a /dev/pts/... path for the PCI serial device."

exec "$qemu" \
    "${machine_args[@]}" \
    -drive "format=raw,file=fat:rw:$esp_dir" \
    -nic none \
    -monitor none \
    -serial stdio \
    -chardev pty,id=pci_serial \
    -device pci-serial,chardev=pci_serial \
    "$@"
