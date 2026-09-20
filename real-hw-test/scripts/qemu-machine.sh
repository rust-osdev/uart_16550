# Per-architecture QEMU machine and firmware selection, shared by run-qemu.sh
# and run-qemu-ci.sh. This file is sourced, not executed.
#
# qemu_machine_setup ARCH QEMU ACCEL VARS_FILE
#
# Validates the firmware for ARCH and fills the machine_args array with the
# machine type, memory, firmware, and input devices. On aarch64 it also places
# a writable copy of the EDK2 variable store at VARS_FILE, whose directory
# must exist. Firmware comes from OVMF, AAVMF_CODE, and AAVMF_VARS; the images
# QEMU ships are the aarch64 default.
qemu_machine_setup() {
    local arch=$1 qemu=$2 accel=$3 vars_file=$4
    case "$arch" in
    x86_64)
        local ovmf=${OVMF:-}
        if [[ -z "$ovmf" ]]; then
            echo "error: OVMF is unset; run 'nix develop' or set OVMF=/path/to/OVMF.fd" \
                >&2
            return 2
        fi
        if [[ ! -r "$ovmf" ]]; then
            echo "error: OVMF firmware is not readable: $ovmf" >&2
            return 2
        fi
        machine_args=(-machine "q35,accel=$accel" -m 256M -bios "$ovmf")
        ;;
    aarch64)
        # QEMU ships pflash-style EDK2 images next to its own installation.
        local share_dir
        share_dir=$(dirname "$(readlink -f "$(command -v "$qemu")")")/../share/qemu
        local aavmf_code=${AAVMF_CODE:-$share_dir/edk2-aarch64-code.fd}
        local aavmf_vars=${AAVMF_VARS:-$share_dir/edk2-arm-vars.fd}
        local firmware
        for firmware in "$aavmf_code" "$aavmf_vars"; do
            if [[ ! -r "$firmware" ]]; then
                echo "error: aarch64 firmware is not readable: $firmware" >&2
                echo "       set AAVMF_CODE and AAVMF_VARS" >&2
                return 2
            fi
        done
        # Writable per-run variable store; the template may be read-only on disk.
        cp "$aavmf_vars" "$vars_file"
        chmod u+w "$vars_file"
        # virt has no built-in keyboard; the UEFI prompts need one.
        machine_args=(-machine "virt,accel=$accel" -cpu max -m 512M
            -drive "if=pflash,format=raw,file=$aavmf_code,readonly=on"
            -drive "if=pflash,format=raw,file=$vars_file"
            -device qemu-xhci -device usb-kbd)
        ;;
    *)
        echo "error: unsupported ARCH '$arch'; supported: x86_64, aarch64" >&2
        return 2
        ;;
    esac
}
