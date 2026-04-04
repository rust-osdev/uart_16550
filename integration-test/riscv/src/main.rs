#![no_main]
#![no_std]

core::arch::global_asm!(include_str!("start.S"));

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use qemu_exit::QEMUExit;
use uart_16550::{Config, Uart16550Tty};

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

const UART_BASE: usize = 0x1000_0000;
const UART_STRIDE: u8 = 1;
const SIFIVE_TEST_BASE: u64 = 0x0010_0000;

#[unsafe(no_mangle)]
extern "C" fn rust_entry() -> ! {
    main()
}

fn exit_qemu(success: bool) -> ! {
    let exit = qemu_exit::RISCV64::new(SIFIVE_TEST_BASE);
    if success {
        exit.exit_success()
    } else {
        exit.exit_failure()
    }
}

fn main() -> ! {
    let uart_base = NonNull::new(UART_BASE as *mut u8).expect("valid MMIO base");

    unsafe {
        let mut uart = Uart16550Tty::new_mmio(uart_base, UART_STRIDE, Config::default())
            .expect("MMIO UART should initialize");
        uart.write_str("hello from serial via riscv mmio")
            .expect("serial write should succeed");
    }

    exit_qemu(true)
}

#[panic_handler]
fn panic_handler(_info: &PanicInfo) -> ! {
    exit_qemu(false);
}
