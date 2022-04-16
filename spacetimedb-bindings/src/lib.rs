mod col_value;
mod col_type;
mod column;
pub use col_value::ColValue;
pub use col_type::ColType;
pub use column::Column;
use std::alloc::{alloc as _alloc, dealloc as _dealloc, Layout};

const ROW_BUF_LEN: usize = 1024;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[no_mangle]
pub unsafe fn alloc(size: usize) -> *mut u8 {
    let align = std::mem::align_of::<usize>();
    let layout = Layout::from_size_align_unchecked(size, align);
    _alloc(layout)
}

#[no_mangle]
pub unsafe fn dealloc(ptr: *mut u8, size: usize) {
    let align = std::mem::align_of::<usize>();
    let layout = Layout::from_size_align_unchecked(size, align);
    _dealloc(ptr, layout);
}

extern "C" {
    fn _create_table(table_id: u32, ptr: *mut u8);
    fn _insert(table_id: u32, ptr: *mut u8);
    fn _iter_next(table_id: u32, ptr: *mut u8);
}

pub fn create_table(table_id: u32, schema: Vec<Column>) {
    unsafe {
        let ptr = alloc(ROW_BUF_LEN);
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
        dealloc(ptr, ROW_BUF_LEN);
    }
}

pub fn insert(table_id: u32, row: Vec<ColValue>) {
    unsafe {
        let ptr = alloc(ROW_BUF_LEN);
        let mut memory = Vec::from_raw_parts(ptr, 0, ROW_BUF_LEN);
        for col in row {
            memory.extend(col.to_data());
        }
        _insert(table_id, ptr);
        dealloc(ptr, ROW_BUF_LEN);
    }
}

// pub fn iter(table_id: u32) -> Option<TableIter> {
//     let ptr = unsafe { alloc(ROW_BUF_LEN) };
//     Some(TableIter {
//         table_id,
//         ptr
//     })
// }

// pub struct TableIter {
//     table_id: u32,
//     ptr: *mut u8
// }

// impl Iterator for TableIter {
//     type Item = Vec<ColValue>;

//     fn next(&mut self) -> Option<Self::Item> {
//         let buf: Vec<u8> = unsafe { Vec::from_raw_parts(self.ptr, 0, ROW_BUF_LEN) };
        
//         return None;
//     }
// }