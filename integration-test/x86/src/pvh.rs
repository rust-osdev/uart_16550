//! Xen PVH entry point.

const XEN_ELFNOTE_PHYS32_ENTRY: u32 = 18;
type Name = [u8; 4];
type CFunc = unsafe extern "C" fn();

#[repr(C, packed(4))]
struct ElfNote<T> {
    name_size: u32,
    desc_size: u32,
    kind: u32,
    name: Name,
    // Payload
    desc: T,
}

// The PVH Boot Protocol starts at the 32-bit entrypoint to our firmware.
unsafe extern "C" {
    fn start();
}

/// Emits an ELF note into the binary which is a valid Xen PVH entry point.
///
/// This is understood by some bootloaders or VMMs (e.g., Cloud Hypervisor) to
/// support direct kernel boot.
#[unsafe(link_section = ".note.xen_pvh")]
#[used]
static PVH_NOTE: ElfNote<CFunc> = ElfNote {
    name_size: size_of::<Name>() as u32,
    desc_size: size_of::<CFunc>() as u32,
    kind: XEN_ELFNOTE_PHYS32_ENTRY,
    name: *b"Xen\0",
    desc: start,
};
