//! Small CPUID helper for the two checks this project needs.

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
compile_error!("This module only supports x86/x86_64.");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuidResult {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Cpuid;

impl Cpuid {
    #[inline]
    pub const fn new() -> Self {
        Self
    }

    #[inline]
    pub fn has_hypervisor_bit(&self) -> bool {
        let res = self.cpuid(0x0000_0001, 0);
        (res.ecx & (1 << 31)) != 0
    }

    #[inline]
    pub fn cpu_brand_contains_qemu(&self) -> bool {
        self.cpu_brand_bytes()
            .map(|brand| contains_subslice(&brand, b"QEMU"))
            .unwrap_or(false)
    }

    #[inline]
    fn cpuid(&self, leaf: u32, subleaf: u32) -> CpuidResult {
        cpuid_count(leaf, subleaf)
    }

    #[inline]
    fn max_extended_leaf(&self) -> u32 {
        self.cpuid(0x8000_0000, 0).eax
    }

    #[inline]
    fn cpu_brand_bytes(&self) -> Option<[u8; 48]> {
        if self.max_extended_leaf() < 0x8000_0004 {
            return None;
        }

        let leaves = [
            self.cpuid(0x8000_0002, 0),
            self.cpuid(0x8000_0003, 0),
            self.cpuid(0x8000_0004, 0),
        ];

        let mut out = [0u8; 48];
        let mut i = 0;

        for leaf in leaves {
            for reg in [leaf.eax, leaf.ebx, leaf.ecx, leaf.edx] {
                out[i..i + 4].copy_from_slice(&reg.to_le_bytes());
                i += 4;
            }
        }

        Some(out)
    }
}

#[inline]
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }

    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[inline]
fn cpuid_count(leaf: u32, subleaf: u32) -> CpuidResult {
    #[cfg(target_arch = "x86")]
    let r = core::arch::x86::__cpuid_count(leaf, subleaf);

    #[cfg(target_arch = "x86_64")]
    let r = core::arch::x86_64::__cpuid_count(leaf, subleaf);

    CpuidResult {
        eax: r.eax,
        ebx: r.ebx,
        ecx: r.ecx,
        edx: r.edx,
    }
}
