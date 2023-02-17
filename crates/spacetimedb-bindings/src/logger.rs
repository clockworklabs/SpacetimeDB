use crate::sys;
use std::sync::Mutex;
use std::{fmt, panic};

#[no_mangle]
extern "C" fn __preinit__00_panic_hook() {
    panic::set_hook(Box::new(panic_hook));
}

fn panic_hook(info: &panic::PanicInfo) {
    let msg = match info.payload().downcast_ref::<&'static str>() {
        Some(s) => *s,
        None => match info.payload().downcast_ref::<String>() {
            Some(s) => &s[..],
            None => "Box<dyn Any>",
        },
    };
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
    // Mutex is fine here because wasm is single-threaded;
    // this is actually basically a RefCell
    buf: Mutex<String>,
}

const MAX_BUF_SIZE: usize = 0x4000; // 16 KiB

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let level = match record.metadata().level() {
            log::Level::Error => sys::LogLevel::Error,
            log::Level::Warn => sys::LogLevel::Warn,
            log::Level::Info => sys::LogLevel::Info,
            log::Level::Debug => sys::LogLevel::Debug,
            log::Level::Trace => sys::LogLevel::Trace,
        };
        let buf = &mut *self.buf.lock().unwrap();
        buf.clear();
        fmt::write(buf, *record.args()).unwrap();
        sys::console_log(level, Some(record.target()), record.file(), record.line(), buf);
        buf.shrink_to(MAX_BUF_SIZE);
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger {
    buf: Mutex::new(String::new()),
};

#[no_mangle]
extern "C" fn __preinit__15_init_log() {
    // if the user wants to set their own logger, that's fine
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
}
