extern crate wee_alloc;
use std::alloc::{alloc as _alloc, dealloc as _dealloc, Layout};

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
}

pub struct Column {
    pub col_type: u32,
    pub col_id: u32,
}

pub struct ColValue {
    pub ty: u32,
    pub value: u32,
}

pub fn create_table(table_id: u32, col_1: Column, col_2: Column, col_3: Column) {
    unsafe {
        let ptr = alloc(1024);
        let mut memory = Vec::from_raw_parts(ptr, 0, 1024);
        let v = col_1.col_type.to_le_bytes();
        for i in 0..v.len() {
            memory.push(v[i]);
        }
        let v = col_1.col_id.to_le_bytes();
        for i in 0..v.len() {
            memory.push(v[i]);
        }
        let v = col_2.col_type.to_le_bytes();
        for i in 0..v.len() {
            memory.push(v[i]);
        }
        let v = col_2.col_id.to_le_bytes();
        for i in 0..v.len() {
            memory.push(v[i]);
        }
        let v = col_3.col_type.to_le_bytes();
        for i in 0..v.len() {
            memory.push(v[i]);
        }
        let v = col_3.col_id.to_le_bytes();
        for i in 0..v.len() {
            memory.push(v[i]);
        }
        _create_table(table_id, ptr);
        dealloc(ptr, 1024);
    }
}

pub fn insert(table_id: u32, col_1: ColValue, col_2: ColValue, col_3: ColValue) {
    unsafe {
        let ptr = alloc(1024);
        let mut memory = Vec::from_raw_parts(ptr, 0, 1024);
        match col_1.ty {
            3 => {
                let v = col_1.value.to_le_bytes();
                for i in 0..v.len() {
                    memory.push(v[i]);
                }
            }
            _ => panic!()
        }
        match col_2.ty {
            3 => {
                let v = col_2.value.to_le_bytes();
                for i in 0..v.len() {
                    memory.push(v[i]);
                }
            }
            _ => panic!()
        }
        match col_3.ty {
            3 => {
                let v = col_3.value.to_le_bytes();
                for i in 0..v.len() {
                    memory.push(v[i]);
                }
            }
            _ => panic!()
        }
        _insert(table_id, ptr);
        dealloc(ptr, 1024);
    }
}