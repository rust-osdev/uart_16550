#![no_main]
#![no_std]

core::arch::global_asm!(include_str!("start.S"), options(att_syntax));

struct DummyGlobalAlloc;

#[global_allocator]
static GLOBAL_ALLOCATOR: DummyGlobalAlloc = DummyGlobalAlloc;

unsafe impl GlobalAlloc for DummyGlobalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        panic!("unsupported! layout={layout:?}");
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        panic!("unsupported! ptr={ptr:?}, layout={layout:?}");
    }
}

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::panic::PanicInfo;
use log::error;
use qemu_exit::QEMUExit;
use uart_16550::{Config, Uart16550Tty};

mod cpuid;
mod debugcon;
mod pvh;

fn runs_inside_qemu() -> bool {
    let cpuid = cpuid::Cpuid::new();
    cpuid.has_hypervisor_bit() && cpuid.cpu_brand_contains_qemu()
}

#[unsafe(no_mangle)]
extern "C" fn rust_entry() -> ! {
    main().expect("Should run kernel");
    unreachable!();
}

fn exit_qemu(success: bool) -> ! {
    let port = 0xf4;
    let exit = qemu_exit::X86::new(port, 73);
    if success {
        exit.exit_success()
    } else {
        exit.exit_failure()
    }
}

fn exit_chv() -> ! {
    unsafe {
        core::arch::asm!(
            "outw %ax, %dx",
            in("ax") 0x34,
            in("dx") 0x600,
            options(att_syntax, noreturn)
        )
    }
}

fn exit_vmm(success: bool) -> ! {
    if runs_inside_qemu() {
        exit_qemu(success);
    } else {
        exit_chv();
    }
}

fn main() -> anyhow::Result<()> {
    debugcon::DebugconLogger::init();

    unsafe {
        let mut uart = Uart16550Tty::new_port(0x3f8, Config::default())?;
        uart.write_str("hello from serial via x86 port I/O")?;
    }

    exit_vmm(true);
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("error: {}", info);
    exit_vmm(false);
}
