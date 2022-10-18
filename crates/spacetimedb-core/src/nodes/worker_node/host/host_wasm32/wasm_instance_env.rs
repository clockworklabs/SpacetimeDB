use std::cell::Cell;

use wasmer::{LazyInit, Memory, MemoryView, NativeFunc, WasmerEnv};

use crate::nodes::worker_node::host::instance_env::InstanceEnv;

type WasmSlice<T> = wasmer::WasmPtr<T, wasmer::Array>;

#[derive(WasmerEnv, Clone)]
pub struct WasmInstanceEnv {
    pub instance_env: InstanceEnv,
    #[wasmer(export)]
    pub memory: LazyInit<Memory>,
    #[wasmer(export)]
    pub alloc: LazyInit<NativeFunc<u32, WasmSlice<u8>>>,
}

/// Wraps an InstanceEnv with the magic necessary to push and pull bytes from webassembly
/// memory.
impl WasmInstanceEnv {
    fn memory(&self) -> &Memory {
        self.memory.get_ref().expect("Initialized memory")
    }
    fn read_output_bytes(memory: &Memory, ptr: WasmSlice<u8>, len: u32) -> bytes::Bytes {
        ptr_get_slice(&memory.view(), ptr, len)
            .expect("invalid pointer")
            .iter()
            .map(Cell::get)
            .collect()
    }

    pub fn console_log(&self, level: u8, ptr: WasmSlice<u8>, len: u32) {
        let memory = self.memory();
        let s = ptr.get_utf8_string(memory, len).unwrap();
        self.instance_env.console_log(level, &s);
    }

    pub fn insert(&self, table_id: u32, ptr: WasmSlice<u8>, len: u32) {
        let buffer = Self::read_output_bytes(self.memory(), ptr, len);
        self.instance_env.insert(table_id, buffer);
    }

    pub fn delete_pk(&self, table_id: u32, ptr: WasmSlice<u8>, len: u32) -> u8 {
        let buffer = Self::read_output_bytes(self.memory(), ptr, len);
        self.instance_env.delete_pk(table_id, buffer)
    }

    pub fn delete_value(&self, table_id: u32, ptr: WasmSlice<u8>, len: u32) -> u8 {
        let buffer = Self::read_output_bytes(self.memory(), ptr, len);
        self.instance_env.delete_value(table_id, buffer)
    }

    pub fn delete_eq(&self, table_id: u32, col_id: u32, ptr: WasmSlice<u8>, len: u32) -> i32 {
        let buffer = Self::read_output_bytes(self.memory(), ptr, len);
        self.instance_env.delete_eq(table_id, col_id, buffer)
    }

    pub fn delete_range(&self, table_id: u32, col_id: u32, ptr: WasmSlice<u8>, len: u32) -> i32 {
        let buffer = Self::read_output_bytes(self.memory(), ptr, len);
        self.instance_env.delete_range(table_id, col_id, buffer)
    }

    pub fn create_table(&self, ptr: WasmSlice<u8>, len: u32) -> u32 {
        let buffer = Self::read_output_bytes(self.memory(), ptr, len);
        self.instance_env.create_table(buffer)
    }

    pub fn get_table_id(&self, ptr: WasmSlice<u8>, len: u32) -> u32 {
        let buffer = Self::read_output_bytes(self.memory(), ptr, len);
        self.instance_env.get_table_id(buffer)
    }

    fn alloc_return_ptr(&self, data: &[u8]) -> u64 {
        let memory = self.memory();
        let alloc_func = self.alloc.get_ref().expect("Intialized alloc function");

        let data_len = data.len() as u32;
        let ptr = alloc_func.call(data_len).unwrap();

        for (dst, src) in ptr_get_slice(&memory.view(), ptr, data_len).unwrap().iter().zip(data) {
            dst.set(*src)
        }

        (ptr.offset() as u64) << 32 | data_len as u64
    }

    pub fn iter(&self, table_id: u32) -> u64 {
        let bytes = self.instance_env.iter(table_id);
        self.alloc_return_ptr(&bytes)
    }
}

fn ptr_get_slice<'a>(memory: &'a MemoryView<u8>, ptr: WasmSlice<u8>, len: u32) -> Option<&'a [Cell<u8>]> {
    memory.get(ptr.offset() as usize..)?.get(..len as usize)
}
