//! UEFI console input and Serial I/O ownership handoff.
//!
//! The test records the firmware baseline, then disconnects serial controllers
//! so firmware and the driver never program a UART concurrently.

use alloc::vec::Vec;
use core::time::Duration;

use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::proto::console::serial::Serial;
use uefi::proto::console::text::Key;
use uefi::{Handle, Status, system};

/// Collects Serial I/O handles, treating an absent protocol as an empty list.
fn serial_handles() -> Result<Vec<Handle>, Status> {
    match boot::locate_handle_buffer(SearchType::from_proto::<Serial>()) {
        Ok(handles) => Ok(handles.iter().copied().collect()),
        Err(error) if error.status() == Status::NOT_FOUND => Ok(Vec::new()),
        Err(error) => Err(error.status()),
    }
}

/// Polls Simple Text Input until local Enter while keeping errors visible.
pub fn wait_for_enter() {
    loop {
        match system::with_stdin(|input| input.read_key()) {
            Ok(Some(Key::Printable(key))) if key == '\r' || key == '\n' => return,
            Ok(_) => boot::stall(Duration::from_millis(20)),
            Err(error) => {
                uefi::println!("WARN: keyboard read failed: {error:?}");
                boot::stall(Duration::from_millis(100));
            }
        }
    }
}

/// Records firmware serial state, then releases every Serial I/O controller.
pub fn disconnect_serial_controllers() -> bool {
    let handles = match serial_handles() {
        Ok(handles) => handles,
        Err(status) => {
            uefi::println!("FAIL: cannot enumerate UEFI SerialIo handles: {status:?}");
            return false;
        }
    };

    uefi::println!("UEFI exposes {} SerialIo handle(s).", handles.len());
    for (index, handle) in handles.iter().copied().enumerate() {
        let params = OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };
        let protocol = {
            // SAFETY: GetProtocol is non-exclusive and dropped before disconnect.
            unsafe { boot::open_protocol::<Serial>(params, OpenProtocolAttributes::GetProtocol) }
        };
        match protocol {
            Ok(serial) => {
                let mode = serial.io_mode();
                uefi::println!(
                    "  [{index}] baud={} data={} parity={:?} stop={:?} timeout={} us fifo={}",
                    mode.baud_rate,
                    mode.data_bits,
                    mode.parity,
                    mode.stop_bits,
                    mode.timeout,
                    mode.receive_fifo_depth
                );
            }
            Err(error) => uefi::println!("  [{index}] mode unavailable: {error:?}"),
        }
    }

    uefi::println!("UEFI SERIAL BASELINE: firmware still owns serial output");
    uefi::println!("Confirm the baseline, set the remote to 9600 8N1, then press Enter.");
    wait_for_enter();

    let mut success = true;
    for (index, handle) in handles.into_iter().enumerate() {
        match boot::disconnect_controller(handle, None, None) {
            Ok(()) => uefi::println!("  [{index}] disconnected"),
            Err(error) => {
                uefi::println!("  [{index}] FAIL: disconnect_controller: {error:?}");
                success = false;
            }
        }
    }

    uefi::println!("UEFI SCREEN CHECK: serial controller disconnection complete");
    success
}
