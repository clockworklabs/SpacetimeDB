#[doc(hidden)]
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => ($crate::log::info!($($arg)*))
}

#[doc(hidden)]
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::log::info!($($arg)*))
}

#[doc(hidden)]
#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => ($crate::log::error!($($arg)*))
}

#[doc(hidden)]
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => ($crate::log::error!($($arg)*))
}

#[macro_export]
macro_rules! dbg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `eprintln!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `eprintln!`
    // will be malformed.
    () => {
        $crate::log::debug!("[{}:{}]", file!(), line!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                $crate::log::debug!("{} = {:#?}",
                    stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
