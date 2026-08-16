#!/usr/bin/env bash
set -euo pipefail

artifact=${1:-build/BOOTX64.EFI}
mount_input=${USB_MOUNT:-}

fail() {
    echo "error: $*" >&2
    exit 2
}

for command in findmnt lsblk install readlink sync; do
    command -v "$command" >/dev/null 2>&1 || fail "required command is missing: $command"
done

[[ -n "$mount_input" ]] || fail \
    "USB_MOUNT is unset; use 'make install USB_MOUNT=/path/to/mounted/efi-partition'"
[[ -r "$artifact" ]] || fail "UEFI artifact is missing: $artifact (run 'make artifact')"

mount_path=$(readlink -f -- "$mount_input") || fail "cannot resolve USB_MOUNT: $mount_input"
[[ "$mount_path" != / ]] || fail "refusing to install into the root filesystem"
[[ -d "$mount_path" ]] || fail "USB_MOUNT is not a directory: $mount_path"
[[ -w "$mount_path" ]] || fail "USB_MOUNT is not writable: $mount_path"

mounted_target=$(findmnt -n -T "$mount_path" -o TARGET) || fail \
    "USB_MOUNT is not on a mounted filesystem: $mount_path"
mounted_target=$(readlink -f -- "$mounted_target") || fail \
    "cannot resolve the filesystem mount point: $mounted_target"
[[ "$mounted_target" == "$mount_path" ]] || fail \
    "USB_MOUNT must be the mount point itself; '$mount_path' is inside '$mounted_target'"

source_name=$(findmnt -n -T "$mount_path" -o SOURCE) || fail \
    "cannot determine the mounted source device"
mount_fstype=$(findmnt -n -T "$mount_path" -o FSTYPE) || fail \
    "cannot determine the mounted filesystem type"
source_name=${source_name%%\[*\]}
source_device=$(readlink -f -- "$source_name") || fail \
    "cannot resolve mounted source device: $source_name"
[[ "$source_device" == /dev/* ]] || fail \
    "mounted source is not a block device: $source_name"

device_type=$(lsblk -dnro TYPE "$source_device")
[[ "$device_type" == part ]] || fail \
    "EFI media must be a partition on a GPT disk; $source_device is type '$device_type'"

parent_name=$(lsblk -dnro PKNAME "$source_device")
[[ -n "$parent_name" ]] || fail "cannot identify the parent disk of $source_device"
parent_device=/dev/$parent_name
partition_table=$(lsblk -dnro PTTYPE "$parent_device")
[[ "$partition_table" == gpt ]] || fail \
    "$parent_device uses '${partition_table:-no recognized partition table}', expected GPT"

block_fstype=$(lsblk -dnro FSTYPE "$source_device")
fat_version=$(lsblk -dnro FSVER "$source_device")
[[ "$mount_fstype" == vfat && "$block_fstype" == vfat ]] || fail \
    "$source_device is '$mount_fstype'/'$block_fstype', expected a mounted FAT filesystem"
[[ "$fat_version" == FAT32 ]] || fail \
    "$source_device reports '${fat_version:-an unknown FAT version}', expected FAT32"

target=$mount_path/EFI/BOOT/BOOTX64.EFI
echo "Installing to validated media:"
echo "  disk:       $parent_device (GPT)"
echo "  partition:  $source_device (FAT32)"
echo "  mount:      $mount_path"
echo "  destination: $target"
install -D -m 0644 -- "$artifact" "$target"
sync "$target"
echo "Installation complete. Unmount the media cleanly before removing it."
