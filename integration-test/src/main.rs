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

mod debugcon;

/// Entry into the Rust code.
#[unsafe(no_mangle)]
extern "C" fn rust_entry() -> ! {
    main().expect("Should run kernel");
    unreachable!();
}

/// Exits QEMU via the shutdown device on the i440fx board.
fn exit_qemu(success: bool) -> ! {
    // configured in Makefile
    let port = 0xf4;
    let exit = qemu_exit::X86::new(port, 73);
    if success {
        exit.exit_success()
    } else {
        exit.exit_failure()
    }
}

/// Executes the kernel's main logic.
fn main() -> anyhow::Result<()> {
    debugcon::DebugconLogger::init();

    // SAFETY: we have exclusive access and the port is valid
    unsafe {
        let mut uart = Uart16550Tty::new_port(0x3f8, Config::default())?;
        uart.write_str("hello from serial via x86 port I/O")?;
    }

    // TODO MMIO test? QEMU doesn't offer this (on x86).

    exit_qemu(true);
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("error: {}", info);
    exit_qemu(false);
}
