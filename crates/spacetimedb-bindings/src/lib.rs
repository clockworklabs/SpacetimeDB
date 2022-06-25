mod data_key;
mod hash;
mod type_def;
mod type_value;
pub use data_key::DataKey;
use std::alloc::{alloc as _alloc, dealloc as _dealloc, Layout};
use std::panic;
pub use type_def::{ElementDef, TupleDef, TypeDef};
pub use type_value::{EqTypeValue, RangeTypeValue, TupleValue, TypeValue};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

extern "C" {
    fn _create_table(table_id: u32, ptr: *mut u8);
    fn _create_index(table_id: u32, col_id: u32, index_type: u8);
    fn _insert(table_id: u32, ptr: *mut u8);
    fn _filter_eq(table_id: u32, col_id: u32, src_ptr: *mut u8, result_ptr: *mut u8);
    fn _delete_eq(table_id: u32, col_id: u32, ptr: *mut u8);
    fn _iter(table_id: u32) -> u64;
    fn _console_log(level: u8, ptr: *const u8, len: u32);
}

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
    ($($arg:tt)*) => ($crate::_console_log_info(&format!($($arg)*)))
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::_console_log_info(&format!($($arg)*)))
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
extern "C" fn alloc(size: usize) -> *mut u8 {
    let align = std::mem::align_of::<usize>();
    unsafe {
        let layout = Layout::from_size_align_unchecked(size, align);
        _alloc(layout)
    }
}

#[no_mangle]
extern "C" fn dealloc(ptr: *mut u8, size: usize) {
    let align = std::mem::align_of::<usize>();
    unsafe {
        let layout = Layout::from_size_align_unchecked(size, align);
        _dealloc(ptr, layout);
    }
}

unsafe fn row_buf() -> *mut u8 {
    if ROW_BUF.is_none() {
        let ptr = alloc(ROW_BUF_LEN);
        ROW_BUF = Some(ptr);
    }
    ROW_BUF.unwrap()
}

pub fn encode_row(row: TupleValue, bytes: &mut Vec<u8>) {
    row.encode(bytes);
}

pub fn decode_row(schema: &TupleDef, bytes: &mut &[u8]) -> (TupleValue, usize) {
    TupleValue::decode(schema, bytes)
}

pub fn encode_schema(schema: TupleDef, bytes: &mut Vec<u8>) {
    schema.encode(bytes);
}

pub fn decode_schema(bytes: &mut &[u8]) -> (TupleDef, usize) {
    TupleDef::decode(bytes)
}

pub fn create_table(table_id: u32, schema: TupleDef) {
    unsafe {
        let ptr = row_buf();
        let mut bytes = Vec::from_raw_parts(ptr, 0, ROW_BUF_LEN);
        schema.encode(&mut bytes);
        std::mem::forget(bytes);
        _create_table(table_id, ptr);
    }
}

pub fn insert(table_id: u32, row: TupleValue) {
    unsafe {
        let ptr = row_buf();
        let mut bytes = Vec::from_raw_parts(ptr, 0, ROW_BUF_LEN);
        row.encode(&mut bytes);
        std::mem::forget(bytes);
        _insert(table_id, ptr);
    }
}

pub fn create_index(_table_id: u32, _index_type: u8, _col_ids: Vec<u32>) {}

// TODO: going to have to somehow ensure TypeValue is equatable
pub fn filter_eq(_table_id: u32, _col_id: u32, _eq_value: TypeValue) -> Option<TupleValue> {
    return None;
}

// pub fn delete_eq(table_id: u32, col_id : u32, eq_value : ColValue) {
//     unsafe {
//         let ptr = row_buf();
//         let mut memory = Vec::from_raw_parts(ptr, 0, ROW_BUF_LEN);
//         memory.extend(eq_value.to_data());
//         _delete_eq(table_id, col_id, ptr);
//     }
// }
//
// pub fn delete_filter() {
//
// }
//
// fn page_table(table_id : u32, pager_token : u32, read_entries : u32) {
//
// }

pub fn __iter__(table_id: u32) -> Option<TableIter> {
    let data = unsafe { _iter(table_id) };
    let ptr = (data >> 32) as u32 as *mut u8;
    let size = data as u32;
    let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(ptr, size as usize, size as usize) };

    let slice = &mut &bytes[..];
    let initial_size = slice.len() as u32;
    let (schema, schema_size) = decode_schema(slice);

    let data_size = (slice.len() - schema_size) as u32;
    let start_ptr = ptr;
    let data_ptr = unsafe { start_ptr.add(schema_size as usize) };

    std::mem::forget(bytes);
    Some(TableIter {
        start_ptr,
        initial_size,
        ptr: data_ptr,
        size: data_size,
        schema,
    })
}

pub struct TableIter {
    start_ptr: *mut u8,
    initial_size: u32,
    ptr: *mut u8,
    size: u32,
    schema: TupleDef,
}

impl Iterator for TableIter {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes: Vec<u8> = unsafe { Vec::from_raw_parts(self.ptr, self.size as usize, self.size as usize) };
        let slice = &mut &bytes[..];
        if slice.len() > 0 {
            let (row, num_read) = decode_row(&self.schema, slice);
            self.ptr = unsafe { self.ptr.add(num_read) };
            self.size = self.size - num_read as u32;
            std::mem::forget(bytes);
            return Some(row);
        }
        // TODO: potential memory leak if they don't read all the stuff, figure out how to do this
        std::mem::forget(bytes);
        dealloc(self.start_ptr, self.initial_size as usize);
        return None;
    }
}
