#!/usr/bin/env bash
set -euo pipefail

artifacts=("$@")
mount_input=${USB_MOUNT:-}

fail() {
    echo "error: $*" >&2
    exit 2
}

for command in findmnt lsblk install readlink sync; do
    command -v "$command" >/dev/null 2>&1 || fail "required command is missing: $command"
done

# Without explicit arguments, deploy every architecture that has been built.
if [[ ${#artifacts[@]} -eq 0 ]]; then
    for artifact in build/BOOT*.EFI; do
        [[ -e "$artifact" ]] && artifacts+=("$artifact")
    done
fi

[[ ${#artifacts[@]} -gt 0 ]] || fail \
    "no UEFI artifacts in build/ (run 'make artifact' or 'make artifacts')"
for artifact in "${artifacts[@]}"; do
    [[ -r "$artifact" ]] || fail "UEFI artifact is missing: $artifact (run 'make artifact')"
done

# lsblk raw output escapes spaces as '\x20'; values are read one at a time, so
# each query returns exactly one field. Media unplugged mid-scan yields an empty
# value, which the callers already treat as unusable.
blk_field() {
    local value
    value=$(lsblk -dnro "$1" -- "$2" 2>/dev/null) || value=
    printf '%b' "$value"
}

# The model is last: it is the least important column and the only one that can
# be truncated without losing information the operator needs to confirm.
format_row() {
    printf '%-14s %6s  %-12s %-24s %-26s %s\n' "$@"
}

# Emits one tab-separated record per usable partition:
# partition, mount point, parent disk, then the display columns.
collect_candidates() {
    local partition type fsver mount parent size label model transport bus
    while read -r partition type; do
        [[ "$type" == part ]] || continue
        [[ "$(blk_field FSTYPE "$partition")" == vfat ]] || continue
        fsver=$(blk_field FSVER "$partition")
        [[ "$fsver" == FAT32 ]] || continue
        # The script never mounts anything, so unmounted media is not offered.
        mount=$(blk_field MOUNTPOINT "$partition")
        [[ -n "$mount" && "$mount" != \[*\] ]] || continue

        parent=$(blk_field PKNAME "$partition")
        [[ -n "$parent" ]] || continue
        parent=/dev/$parent
        [[ "$(blk_field PTTYPE "$parent")" == gpt ]] || continue

        # Built-in disks carry the host's own ESP, and overwriting EFI/BOOT
        # there breaks the host's boot path. Only removable media is listed;
        # USB_MOUNT remains the way to reach anything else.
        transport=$(blk_field TRAN "$parent")
        [[ "$transport" == usb || "$(blk_field RM "$partition")" == 1 ||
            "$(blk_field HOTPLUG "$partition")" == 1 ]] || continue

        size=$(blk_field SIZE "$partition")
        label=$(blk_field LABEL "$partition")
        model=$(blk_field MODEL "$parent")
        bus=${transport:-unknown}
        [[ -w "$mount" ]] || bus="$bus, read-only"
        printf '%s\t%s\t%s\t' "$partition" "$mount" "$parent"
        format_row \
            "$partition" "$size" "${label:--}" "$bus" "$mount" "${model:--}"
    done < <(lsblk -nrpo NAME,TYPE)
}

# Picks a mount point interactively and echoes it. The picker draws on the
# terminal and diagnostics go to stderr, so only the result reaches the caller.
# It runs in a command substitution: 'exit' ends that subshell, and errexit in
# the caller turns the failed assignment into the script's own exit.
select_usb_mount() {
    local candidates=() selection mount header
    mapfile -t candidates < <(collect_candidates)
    if [[ ${#candidates[@]} -eq 0 ]]; then
        echo "error: no removable FAT32 partition on a GPT disk is mounted" >&2
        echo "       mount the EFI partition first, or name it with USB_MOUNT" >&2
        exit 2
    fi

    header=$'Enter installs, Esc aborts\n'
    header+=$(format_row DEVICE SIZE LABEL BUS MOUNT MODEL)

    selection=$(printf '%s\n' "${candidates[@]}" | fzf \
        --delimiter=$'\t' \
        --with-nth=4.. \
        --height=~60% \
        --border \
        --border-label=' Select the EFI system partition ' \
        --header="$header" \
        --header-first \
        --preview='lsblk -o NAME,SIZE,TYPE,FSTYPE,FSVER,LABEL,MOUNTPOINTS -- {3}' \
        --preview-window='down,8,border-top') || selection=

    [[ -n "$selection" ]] || fail "no media selected"
    IFS=$'\t' read -r _ mount _ <<<"$selection"
    printf '%s\n' "$mount"
}

if [[ -z "$mount_input" ]]; then
    # The device node can exist without a controlling terminal, so the guard
    # opens it instead of only testing its permissions.
    if { : </dev/tty; } 2>/dev/null && command -v fzf >/dev/null 2>&1; then
        mount_input=$(select_usb_mount)
    else
        fail "USB_MOUNT is unset; use 'make install USB_MOUNT=/path/to/mounted/efi-partition'
       (interactive selection needs a terminal and fzf)"
    fi
fi

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

echo "Installing to validated media:"
echo "  disk:       $parent_device (GPT)"
echo "  partition:  $source_device (FAT32)"
echo "  mount:      $mount_path"
# Each artifact already carries its architecture's removable-media file name.
for artifact in "${artifacts[@]}"; do
    target=$mount_path/EFI/BOOT/$(basename "$artifact")
    echo "  destination: $target"
    install -D -m 0644 -- "$artifact" "$target"
    sync "$target"
done
echo "Installation complete. Unmount the media cleanly before removing it."
