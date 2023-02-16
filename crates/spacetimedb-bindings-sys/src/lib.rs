extern crate alloc;

#[macro_use]
mod errno;

use core::fmt;
use core::mem::MaybeUninit;
use core::num::NonZeroU16;

use alloc::boxed::Box;

pub const ABI_VERSION: u16 = 1;

pub mod raw {
    #[link(wasm_import_module = "spacetime")]
    extern "C" {
        pub fn _create_table(
            name: *const u8,
            name_len: usize,
            schema: *const u8,
            schema_len: usize,
            out: *mut u32,
        ) -> u16;
        pub fn _get_table_id(name: *const u8, name_len: usize, out: *mut u32) -> u16;
        // pub fn _create_index(table_id: u32, col_id: u32, index_type: u8);

        pub fn _insert(table_id: u32, row: *const u8, row_len: usize) -> u16;

        pub fn _delete_pk(table_id: u32, pk: *const u8, pk_len: usize) -> u16;
        pub fn _delete_value(table_id: u32, row: *const u8, row_len: usize) -> u16;
        pub fn _delete_eq(table_id: u32, col_id: u32, value: *const u8, value_len: usize, out: *mut u32) -> u16;
        pub fn _delete_range(
            table_id: u32,
            col_id: u32,
            range_start: *const u8,
            range_start_len: usize,
            range_end: *const u8,
            range_end_len: usize,
            out: *mut u32,
        ) -> u16;

        // pub fn _filter_eq(table_id: u32, col_id: u32, src_ptr: *const u8, result_ptr: *const u8);

        pub fn _iter(table_id: u32, out: *mut Buffer) -> u16;
        pub fn _console_log(level: u8, text: *const u8, text_len: usize);

        pub fn _schedule_reducer(name: *const u8, name_len: usize, args: *const u8, args_len: usize, time: u64);

        pub fn _buffer_len(bufh: Buffer) -> usize;
        pub fn _buffer_consume(bufh: Buffer, into: *mut u8, len: usize);
        pub fn _buffer_alloc(data: *const u8, data_len: usize) -> Buffer;
    }

    #[repr(transparent)]
    pub struct Buffer {
        pub raw: u32,
    }
    pub const INVALID_BUFFER: Buffer = Buffer { raw: u32::MAX };

    pub type DescriptorFunc = extern "C" fn() -> Buffer;
    pub type InitFunc = extern "C" fn() -> Buffer;
    pub type ReducerFunc = extern "C" fn(sender: Buffer, timestamp: u64, args: Buffer) -> Buffer;
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Errno(NonZeroU16);

// once Error gets exposed from core this crate can be no_std again
impl std::error::Error for Errno {}

macro_rules! def_errno {
    ($($name:ident => $desc:literal,)*) => {
        impl Errno {
            $(#[doc = $desc] pub const $name: Errno = Errno(unsafe { NonZeroU16::new_unchecked(errno::$name) });)*
        }
        const fn strerror(err: Errno) -> Option<&'static str> {
            match err {
                $(Errno::$name => Some($desc),)*
                _ => None,
            }
        }
    };
}
errnos!(def_errno);

impl Errno {
    /// Get a description of the errno value, if it has one
    pub const fn message(self) -> Option<&'static str> {
        strerror(self)
    }
    #[inline]
    pub const fn from_code(code: u16) -> Option<Self> {
        match NonZeroU16::new(code) {
            Some(code) => Some(Errno(code)),
            None => None,
        }
    }
    #[inline]
    pub const fn code(self) -> u16 {
        self.0.get()
    }
}

impl fmt::Debug for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut fmt = f.debug_struct("Errno");
        fmt.field("code", &self.code());
        if let Some(msg) = self.message() {
            fmt.field("message", &msg);
        }
        fmt.finish()
    }
}
impl fmt::Display for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = self.message().unwrap_or("Unknown error");
        write!(f, "{message} (error {})", self.code())
    }
}

fn cvt(x: u16) -> Result<(), Errno> {
    match Errno::from_code(x) {
        None => Ok(()),
        Some(err) => Err(err),
    }
}

#[inline]
unsafe fn call<T>(f: impl FnOnce(*mut T) -> u16) -> Result<T, Errno> {
    let mut ret = MaybeUninit::uninit();
    cvt(f(ret.as_mut_ptr()))?;
    Ok(ret.assume_init())
}

#[inline]
pub fn create_table(name: &str, schema: &[u8]) -> Result<u32, Errno> {
    unsafe { call(|out| raw::_create_table(name.as_ptr(), name.len(), schema.as_ptr(), schema.len(), out)) }
}
#[inline]
pub fn get_table_id(name: &str) -> Result<u32, Errno> {
    unsafe { call(|out| raw::_get_table_id(name.as_ptr(), name.len(), out)) }
}
// #[inline]
// pub fn create_index(table_id: u32, col_id: u32, index_type: u8) {
//     unsafe { raw::_create_index(table_id, col_id, index_type) }
// }

#[inline]
pub fn insert(table_id: u32, row: &[u8]) -> Result<(), Errno> {
    cvt(unsafe { raw::_insert(table_id, row.as_ptr(), row.len()) })
}

#[inline]
pub fn delete_pk(table_id: u32, pk: &[u8]) -> Result<(), Errno> {
    cvt(unsafe { raw::_delete_pk(table_id, pk.as_ptr(), pk.len()) })
}
#[inline]
pub fn delete_value(table_id: u32, row: &[u8]) -> Result<(), Errno> {
    cvt(unsafe { raw::_delete_value(table_id, row.as_ptr(), row.len()) })
}
#[inline]
pub fn delete_eq(table_id: u32, col_id: u32, value: &[u8]) -> Result<u32, Errno> {
    unsafe { call(|out| raw::_delete_eq(table_id, col_id, value.as_ptr(), value.len(), out)) }
}
#[inline]
pub fn delete_range(table_id: u32, col_id: u32, range_start: &[u8], range_end: &[u8]) -> Result<u32, Errno> {
    unsafe {
        call(|out| {
            raw::_delete_range(
                table_id,
                col_id,
                range_start.as_ptr(),
                range_start.len(),
                range_end.as_ptr(),
                range_end.len(),
                out,
            )
        })
    }
}

// not yet implemented
// #[inline]
// pub fn filter_eq(table_id: u32, col_id: u32, src_ptr: *mut u8, result_ptr: *mut u8) {}

#[inline]
pub fn iter(table_id: u32) -> Result<Box<[u8]>, Errno> {
    unsafe {
        let buf = call(|out| raw::_iter(table_id, out))?;
        Ok(buf.read())
    }
}
#[inline]
pub fn console_log(level: u8, text: &[u8]) {
    unsafe { raw::_console_log(level, text.as_ptr(), text.len()) }
}

/// not fully implemented yet
#[inline]
pub fn schedule(name: &str, args: &[u8], time: u64) {
    unsafe { raw::_schedule_reducer(name.as_ptr(), name.len(), args.as_ptr(), args.len(), time) }
}

pub use raw::Buffer;

impl Buffer {
    pub fn data_len(&self) -> usize {
        unsafe { raw::_buffer_len(Buffer { raw: self.raw }) }
    }

    pub fn read(self) -> Box<[u8]> {
        let len = self.data_len();
        let mut buf = alloc::vec::Vec::with_capacity(len);
        self.read_uninit(buf.spare_capacity_mut());
        unsafe { buf.set_len(len) };
        buf.into_boxed_slice()
    }

    /// if the length is wrong the module will crash
    pub fn read_array<const N: usize>(self) -> [u8; N] {
        // use MaybeUninit::uninit_array once stable
        let mut arr = unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() };
        self.read_uninit(&mut arr);
        // use MaybeUninit::array_assume_init once stable
        unsafe { (&arr as *const [_; N]).cast::<[u8; N]>().read() }
    }

    pub fn read_uninit(self, buf: &mut [MaybeUninit<u8>]) {
        unsafe { raw::_buffer_consume(self, buf.as_mut_ptr().cast(), buf.len()) }
    }

    pub fn alloc(data: &[u8]) -> Self {
        unsafe { raw::_buffer_alloc(data.as_ptr(), data.len()) }
    }

    pub fn is_invalid(&self) -> bool {
        self.raw == raw::INVALID_BUFFER.raw
    }
}

// TODO: eventually there should be a way to set a consistent random seed for a module
#[cfg(feature = "getrandom")]
fn fake_random(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    for i in 0..buf.len() {
        let start = match i % 4 {
            0 => 0x64,
            1 => 0xe9,
            2 => 0x48,
            _ => 0xb5,
        };
        buf[i] = (start ^ i) as u8;
    }

    Result::Ok(())
}
#[cfg(feature = "getrandom")]
getrandom::register_custom_getrandom!(fake_random);
