#!/usr/bin/env bash
set -euo pipefail

arch=${ARCH:-x86_64}
qemu=${QEMU:-qemu-system-$arch}
accel=${QEMU_ACCEL:-kvm}
ovmf=${OVMF:-}
esp_dir=${ESP_DIR:-../target/real-hw-test/qemu-esp-$arch}
artifact=${ARTIFACT:-}

if ! command -v "$qemu" >/dev/null 2>&1; then
    echo "error: QEMU executable not found: $qemu" >&2
    exit 2
fi

# Per-architecture machine, firmware, display, and input configuration.
case "$arch" in
x86_64)
    artifact=${artifact:-build/BOOTX64.EFI}
    if [[ -z "$ovmf" ]]; then
        echo "error: OVMF is unset; run 'nix develop' or set OVMF=/path/to/OVMF.fd" \
            >&2
        exit 2
    fi
    if [[ ! -r "$ovmf" ]]; then
        echo "error: OVMF firmware is not readable: $ovmf" >&2
        exit 2
    fi
    machine_args=(-machine "q35,accel=$accel" -m 256M -bios "$ovmf")
    ;;
aarch64)
    artifact=${artifact:-build/BOOTAA64.EFI}
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
    # virt has no built-in display or keyboard; ramfb and a USB keyboard give
    # the operator the UEFI monitor and Enter/Escape navigation.
    machine_args=(-machine "virt,accel=$accel" -cpu max -m 512M
        -drive "if=pflash,format=raw,file=$aavmf_code,readonly=on"
        -drive "if=pflash,format=raw,file=$esp_dir-vars.fd"
        -device ramfb -device qemu-xhci -device usb-kbd)
    ;;
*)
    echo "error: unsupported ARCH '$arch'; supported: x86_64, aarch64" >&2
    exit 2
    ;;
esac

if [[ ! -r "$artifact" ]]; then
    echo "error: UEFI artifact is missing: $artifact (run 'make artifact')" >&2
    exit 2
fi

# Recreate the virtual ESP so QEMU never boots a stale application.
rm -rf "$esp_dir"
mkdir -p "$esp_dir/EFI/BOOT"
cp "$artifact" "$esp_dir/EFI/BOOT/$(basename "$artifact")"
if [[ "$arch" == aarch64 ]]; then
    # Writable per-run variable store; the template may be read-only on disk.
    cp "$aavmf_vars" "$esp_dir-vars.fd"
    chmod u+w "$esp_dir-vars.fd"
fi

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
