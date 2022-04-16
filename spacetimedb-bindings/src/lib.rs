mod col_value;
mod col_type;
mod column;
pub use col_value::ColValue;
pub use col_type::ColType;
pub use column::Column;
use std::ffi::CString;
use std::os::raw::c_char;
use std::alloc::{alloc as _alloc, dealloc as _dealloc, Layout};
use std::panic;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

extern "C" {
    fn _create_table(table_id: u32, ptr: *mut u8);
    fn _insert(table_id: u32, ptr: *mut u8);
    fn _iter_next(table_id: u32, ptr: *mut u8);
    fn _console_log(level: u8, ptr: *const c_char);
}

// TODO: probably do something lighter weight here
#[no_mangle]
extern fn _init() {
    panic::set_hook(Box::new(panic_hook));
}

fn panic_hook(info: &panic::PanicInfo) {
    let msg = info.to_string();
    eprintln!("{}", msg);
}

#[doc(hidden)]
pub fn _console_log_debug(string: String) {
    let s = CString::new(string).unwrap();
    let ptr = s.as_ptr();
    unsafe { _console_log(3, ptr); }
}

#[doc(hidden)]
pub fn _console_log_info(string: String) {
    let s = CString::new(string).unwrap();
    let ptr = s.as_ptr();
    unsafe { _console_log(2, ptr); }
}

#[doc(hidden)]
pub fn _console_log_warn(string: &str) {
    let s = CString::new(string).unwrap();
    let ptr = s.as_ptr();
    unsafe { _console_log(1, ptr); }
}

#[doc(hidden)]
pub fn _console_log_error(string: &str) {
    let s = CString::new(string).unwrap();
    let ptr = s.as_ptr();
    unsafe { _console_log(0, ptr); }
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => ($crate::_console_log_info(format!($($arg)*)))
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::_console_log_info(format!($($arg)*)))
}

#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => ($crate::_console_log_error(&format!($($arg)*)))
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => ($crate::_console_log_error(&format!($($arg)*)))
}

#[macro_export]
macro_rules! dbg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `eprintln!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `eprintln!`
    // will be malformed.
    () => {
        $crate::eprintln!("[{}:{}]", file!(), line!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                $crate::eprintln!("[{}:{}] {} = {:#?}",
                    file!(), line!(), stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}

const ROW_BUF_LEN: usize = 1024;
static mut ROW_BUF: Option<*mut u8> = None;

#[no_mangle]
unsafe extern fn alloc(size: usize) -> *mut u8 {
    let align = std::mem::align_of::<usize>();
    let layout = Layout::from_size_align_unchecked(size, align);
    _alloc(layout)
}

#[no_mangle]
unsafe extern fn dealloc(ptr: *mut u8, size: usize) {
    let align = std::mem::align_of::<usize>();
    let layout = Layout::from_size_align_unchecked(size, align);
    _dealloc(ptr, layout);
}

fn row_buf() -> *mut u8 {
    unsafe {
        if ROW_BUF.is_none() {
            let ptr = alloc(ROW_BUF_LEN);
            ROW_BUF = Some(ptr);
        }
        ROW_BUF.unwrap()
    }
}

pub fn create_table(table_id: u32, schema: Vec<Column>) {
    unsafe {
        let ptr = row_buf();
        let mut memory = Vec::from_raw_parts(ptr, 0, ROW_BUF_LEN);
        for col in schema {
            let v = col.col_type.to_u32().to_le_bytes();
            for i in 0..v.len() {
                memory.push(v[i]);
            }
            let v = col.col_id.to_le_bytes();
            for i in 0..v.len() {
                memory.push(v[i]);
            }
        }
        _create_table(table_id, ptr);
    }
}

pub fn insert(table_id: u32, row: Vec<ColValue>) {
    unsafe {
        let ptr = row_buf();
        let mut memory = Vec::from_raw_parts(ptr, 0, ROW_BUF_LEN);
        for col in row {
            memory.extend(col.to_data());
        }
        _insert(table_id, ptr);
    }
}

pub fn iter(table_id: u32) -> Option<TableIter> {
    let ptr = unsafe { alloc(ROW_BUF_LEN) };
    Some(TableIter {
        table_id,
        ptr
    })
}

pub struct TableIter {
    table_id: u32,
    ptr: *mut u8
}

impl Iterator for TableIter {
    type Item = Vec<ColValue>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe { _iter_next(self.table_id, self.ptr); }
        let _buf: Vec<u8> = unsafe { Vec::from_raw_parts(self.ptr, 0, ROW_BUF_LEN) };
        
        return None;
    }
}