//! Defines our panic hook and that `log` will log to the console.

use crate::sys;
use std::sync::Mutex;
use std::{fmt, panic};

/// Registers the panic hook to our own.
#[no_mangle]
extern "C" fn __preinit__00_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}

/// Our own panic hook logging to the console.
fn panic_hook(info: &panic::PanicHookInfo) {
    // Try to look into some string types we know (`&'static str` and `String`).
    let msg = match info.payload().downcast_ref::<&'static str>() {
        Some(s) => *s,
        None => match info.payload().downcast_ref::<String>() {
            Some(s) => &s[..],
            None => "Box<dyn Any>",
        },
    };

    // Log the panic message to the console.
    let location = info.location();
    sys::console_log(
        sys::LogLevel::Panic,
        None,
        location.map(|l| l.file()),
        location.map(|l| l.line()),
        msg,
    )
}

struct Logger {
    // `Mutex` is fine here because WASM is single-threaded;
    // this is actually basically a `RefCell`
    buf: Mutex<String>,
}

const MAX_BUF_SIZE: usize = 0x4000; // 16 KiB

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        // Translate `log`'s levels to `sys`.
        let level = match record.metadata().level() {
            log::Level::Error => sys::LogLevel::Error,
            log::Level::Warn => sys::LogLevel::Warn,
            log::Level::Info => sys::LogLevel::Info,
            log::Level::Debug => sys::LogLevel::Debug,
            log::Level::Trace => sys::LogLevel::Trace,
        };

        // Write the message to the buffer.
        let buf = &mut *self.buf.lock().unwrap();
        buf.clear();
        fmt::write(buf, *record.args()).unwrap();

        // Log the buffer to the console.
        sys::console_log(level, Some(record.target()), record.file(), record.line(), buf);

        // If we allocated above `MAX_BUF_SIZE`, make sure we shrink below it.
        buf.shrink_to(MAX_BUF_SIZE);
    }

    fn flush(&self) {}
}

/// Stores the buffer used for logging.
static LOGGER: Logger = Logger {
    buf: Mutex::new(String::new()),
};

/// Registers our logger unless already set.
/// The maximum level is `Trace`.
#[no_mangle]
extern "C" fn __preinit__15_init_log() {
    // if the user wants to set their own logger, that's fine
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
}
