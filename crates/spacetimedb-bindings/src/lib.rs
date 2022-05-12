mod col_type;
mod col_value;
mod column;
mod schema;
pub use col_type::ColType;
pub use col_value::ColValue;
pub use column::Column;
pub use schema::Schema;
use std::alloc::{alloc as _alloc, dealloc as _dealloc, Layout};
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

extern "C" {
    fn _create_table(table_id: u32, ptr: *mut u8);
    fn _insert(table_id: u32, ptr: *mut u8);
    fn _iter(table_id: u32) -> u64;
    fn _console_log(level: u8, ptr: *const c_char);
}

// TODO: probably do something lighter weight here
#[no_mangle]
extern "C" fn __init() {
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
    unsafe {
        _console_log(3, ptr);
    }
}

#[doc(hidden)]
pub fn _console_log_info(string: String) {
    let s = CString::new(string).unwrap();
    let ptr = s.as_ptr();
    unsafe {
        _console_log(2, ptr);
    }
}

#[doc(hidden)]
pub fn _console_log_warn(string: &str) {
    let s = CString::new(string).unwrap();
    let ptr = s.as_ptr();
    unsafe {
        _console_log(1, ptr);
    }
}

#[doc(hidden)]
pub fn _console_log_error(string: &str) {
    let s = CString::new(string).unwrap();
    let ptr = s.as_ptr();
    unsafe {
        _console_log(0, ptr);
    }
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
unsafe extern "C" fn alloc(size: usize) -> *mut u8 {
    let align = std::mem::align_of::<usize>();
    let layout = Layout::from_size_align_unchecked(size, align);
    _alloc(layout)
}

#[no_mangle]
unsafe extern "C" fn dealloc(ptr: *mut u8, size: usize) {
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

pub fn encode_row(row: Vec<ColValue>, bytes: &mut Vec<u8>) {
    for col in row {
        bytes.extend(col.to_data());
    }
}

pub fn decode_row(columns: &Vec<Column>, bytes: &mut &[u8]) -> (Vec<ColValue>, usize) {
    let mut row = Vec::new();
    let mut total_read = 0;
    for col in columns {
        row.push(ColValue::from_data(&col.col_type, bytes));
        let num_read = col.col_type.size() as usize;
        total_read += num_read;
        *bytes = &bytes[num_read..];
    }
    (row, total_read)
}

pub fn encode_schema(schema: Schema, bytes: &mut Vec<u8>) {
    bytes.push(schema.columns.len() as u8);
    for col in schema.columns {
        let v = col.col_type.to_u32().to_le_bytes();
        for i in 0..v.len() {
            bytes.push(v[i]);
        }
        let v = col.col_id.to_le_bytes();
        for i in 0..v.len() {
            bytes.push(v[i]);
        }
    }
}

pub fn decode_schema(bytes: &mut &[u8]) -> Schema {
    let mut columns: Vec<Column> = Vec::new();
    let len = bytes[0];
    *bytes = &bytes[1..];
    for _ in 0..len {
        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        *bytes = &bytes[4..];
        let col_type = ColType::from_u32(u32::from_le_bytes(dst));

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        *bytes = &bytes[4..];
        let col_id = u32::from_le_bytes(dst);

        columns.push(Column { col_type, col_id });
    }
    Schema { columns }
}

pub fn create_table(table_id: u32, schema: Vec<Column>) {
    unsafe {
        let ptr = row_buf();
        let mut memory = Vec::from_raw_parts(ptr, 0, ROW_BUF_LEN);
        encode_schema(Schema { columns: schema }, &mut memory);
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
    let data = unsafe { _iter(table_id) };
    let ptr = (data >> 32) as u32 as *mut u8;
    let size = data as u32;
    let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(ptr, size as usize, size as usize) };
    let slice = &mut &bytes[..];
    let start_size = slice.len() as u32;
    let schema = decode_schema(slice);
    let size = slice.len() as u32;
    let start_ptr = ptr;
    let ptr = unsafe { start_ptr.add((start_size - size) as usize) };
    std::mem::forget(bytes);
    Some(TableIter {
        start_ptr: ptr,
        start_size,
        ptr,
        size,
        schema,
    })
}

pub struct TableIter {
    start_ptr: *mut u8,
    start_size: u32,
    ptr: *mut u8,
    size: u32,
    schema: Schema,
}

impl Iterator for TableIter {
    type Item = Vec<ColValue>;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(self.ptr, self.size as usize, self.size as usize) };
        let slice = &mut &bytes[..];
        if slice.len() > 0 {
            let (row, num_read) = decode_row(&self.schema.columns, slice);
            self.ptr = unsafe { self.ptr.add(num_read) };
            self.size = self.size - num_read as u32;
            std::mem::forget(bytes);
            return Some(row);
        }
        // TODO: potential memory leak if they don't read all the stuff, figure out how to do this
        std::mem::forget(bytes);
        unsafe { dealloc(self.start_ptr, self.start_size as usize) };
        return None;
    }
}
