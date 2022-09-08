use crate::_console_log;
use std::panic;

// TODO: probably do something lighter weight here
#[no_mangle]
extern "C" fn __init_panic__() {
    panic::set_hook(Box::new(panic_hook));
}

fn panic_hook(info: &panic::PanicInfo) {
    let msg = info.to_string();
    eprintln!("{}", msg);
}

#[doc(hidden)]
pub fn _console_log_debug(string: &str) {
    let s = string.as_bytes();
    let ptr = s.as_ptr();
    unsafe {
        _console_log(3, ptr, s.len() as u32);
    }
}

#[doc(hidden)]
pub fn _console_log_info(string: &str) {
    let s = string.as_bytes();
    let ptr = s.as_ptr();
    unsafe {
        _console_log(2, ptr, s.len() as u32);
    }
}

#[doc(hidden)]
pub fn _console_log_warn(string: &str) {
    let s = string.as_bytes();
    let ptr = s.as_ptr();
    unsafe {
        _console_log(1, ptr, s.len() as u32);
    }
}

#[doc(hidden)]
pub fn _console_log_error(string: &str) {
    let s = string.as_bytes();
    let ptr = s.as_ptr();
    unsafe {
        _console_log(0, ptr, s.len() as u32);
    }
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => ($crate::io::_console_log_info(&format!($($arg)*)))
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::io::_console_log_info(&format!($($arg)*)))
}

#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => ($crate::io::_console_log_error(&format!($($arg)*)))
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => ($crate::io::_console_log_error(&format!($($arg)*)))
}

#[macro_export]
macro_rules! dbg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `eprintln!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `eprintln!`
    // will be malformed.
    () => {
        $crate::io::eprintln!("[{}:{}]", file!(), line!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                $crate::io::eprintln!("[{}:{}] {} = {:#?}",
                    file!(), line!(), stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::io::dbg!($val)),+,)
    };
}
