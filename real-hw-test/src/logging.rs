//! Fail-closed screen and file diagnostics for one integration-test run.
//!
//! Persisting the screen transcript makes a physical-hardware failure
//! inspectable after reboot. A write failure aborts instead of silently losing
//! diagnostics that are needed to interpret the hardware result.

use alloc::format;
use alloc::string::String;
use core::cell::UnsafeCell;
use core::fmt::{Arguments, Write};

use jiff::civil::DateTime;
use uefi::boot;
use uefi::fs::PathBuf;
use uefi::proto::media::file::{File, FileAttribute, FileMode, RegularFile};
use uefi::runtime;

/// Owns the opened log file and flushes each diagnostic before displaying it.
struct Logger {
    file: RegularFile,
}

/// Holds the single logger used by this synchronous, interrupt-free test.
struct LoggerSlot(UnsafeCell<Option<Logger>>);

// SAFETY: The test is synchronous and deliberately does not enable interrupts,
// so no concurrent caller can access the logger.
unsafe impl Sync for LoggerSlot {}

/// Stores the logger after initialization and before the first test diagnostic.
static LOGGER: LoggerSlot = LoggerSlot(UnsafeCell::new(None));

/// Creates the dated log file on the volume that contains this UEFI image.
pub fn init() -> Result<(), &'static str> {
    let time = runtime::get_time().map_err(|_| "could not read UEFI time")?;
    let time = DateTime::try_from(time).map_err(|_| "UEFI time is invalid")?;
    let file_name = format!(
        "uart_16550_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.txt",
        time.year(),
        time.month(),
        time.day(),
        time.hour(),
        time.minute(),
        time.second(),
    );
    let file_name =
        uefi::CString16::try_from(file_name.as_str()).map_err(|_| "log path is invalid")?;
    let mut protocol = boot::get_image_file_system(boot::image_handle())
        .map_err(|_| "could not open image file system")?;
    let directory = PathBuf::from(uefi::cstr16!("/uart_16550_test_logs"));
    let directory: &uefi::CStr16 = directory.as_ref();
    let mut root = protocol
        .open_volume()
        .map_err(|_| "could not open image volume")?;
    let mut directory = match root.open(directory, FileMode::ReadWrite, FileAttribute::empty()) {
        Ok(handle) => handle,
        Err(_) => root
            .open(
                directory,
                FileMode::CreateReadWrite,
                FileAttribute::DIRECTORY,
            )
            .map_err(|_| "could not create /uart_16550_test_logs")?,
    }
    .into_directory()
    .ok_or("/uart_16550_test_logs is not a directory")?;
    let file = directory
        .open(
            file_name.as_ref(),
            FileMode::CreateReadWrite,
            FileAttribute::empty(),
        )
        .map_err(|_| "could not create test log file")?
        .into_regular_file()
        .ok_or("test log path is not a regular file")?;

    // SAFETY: Initialization runs once before any test diagnostics are emitted.
    unsafe { *LOGGER.0.get() = Some(Logger::new(file)) };
    Ok(())
}

impl Logger {
    /// Retains one file handle so each write extends the same run transcript.
    fn new(file: RegularFile) -> Self {
        Self { file }
    }

    /// Appends one formatted line and flushes it to FAT before console output.
    fn write_line(&mut self, args: Arguments<'_>) -> Result<(), &'static str> {
        let mut line = String::new();
        line.write_fmt(args)
            .map_err(|_| "could not format test diagnostic")?;
        line.push('\n');
        self.file
            .write(line.as_bytes())
            .map_err(|_| "could not write test log file")?;
        self.file
            .flush()
            .map_err(|_| "could not flush test log file")
    }
}

/// Writes a diagnostic to persistent storage first, then displays it on screen.
pub fn println(args: Arguments<'_>) {
    // SAFETY: The test runs synchronously and `init` installs the sole logger.
    let logger = unsafe { (&mut *LOGGER.0.get()).as_mut() };
    let Some(logger) = logger else {
        uefi_rs::println!("CRITICAL: test logger was not initialized");
        panic!("test logger was not initialized");
    };
    if let Err(error) = logger.write_line(args) {
        uefi_rs::println!("CRITICAL: {error}; aborting test");
        panic!("test log write failed");
    }
    uefi_rs::println!("{}", args);
}
