use wasmer::{Array, LazyInit, Memory, NativeFunc, WasmerEnv, WasmPtr};

use crate::nodes::worker_node::host::instance_env::InstanceEnv;

#[derive(WasmerEnv, Clone)]
pub struct WasmInstanceEnv {
    pub instance_env: InstanceEnv,
    #[wasmer(export)]
    pub memory: LazyInit<Memory>,
    #[wasmer(export)]
    pub alloc: LazyInit<NativeFunc<u32, WasmPtr<u8, Array>>>,
}

/// Wraps an InstanceEnv with the magic necessary to push and pull bytes from webassembly
/// memory.
impl WasmInstanceEnv {
    fn bytes_to_string(memory: &Memory, ptr: u32, len: u32) -> String {
        let view = memory.view::<u8>();
        let start = ptr as usize;
        let end = start + len as usize;
        let mut bytes = Vec::new();
        for c in view[start..end].iter() {
            let v = c.get();
            bytes.push(v);
        }
        String::from_utf8(bytes).unwrap()
    }

    fn read_output_bytes(memory: &Memory, ptr: u32) -> bytes::Bytes {
        const ROW_BUF_LEN: usize = 1024 * 1024;
        let view = memory.view::<u8>();
        let start = ptr as usize;
        let end = ptr as usize + ROW_BUF_LEN;
        view[start..end].iter().map(|c| c.get()).collect::<bytes::Bytes>()
    }

    pub fn console_log(&self, level: u8, ptr: u32, len: u32) {
        let memory = self.memory.get_ref().expect("Initialized memory");
        let s = Self::bytes_to_string(memory, ptr, len);
        self.instance_env.console_log(level, &s);
    }

    pub fn insert(&self, table_id: u32, ptr: u32) {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);
        self.instance_env.insert(table_id, buffer);
    }

    pub fn delete_pk(&self, table_id: u32, ptr: u32) -> u8 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);
        self.instance_env.delete_pk(table_id, buffer)
    }

    pub fn delete_value(&self, table_id: u32, ptr: u32) -> u8 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);
        self.instance_env.delete_value(table_id, buffer)
    }

    pub fn delete_eq(&self, table_id: u32, col_id: u32, ptr: u32) -> i32 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);
        self.instance_env.delete_eq(table_id, col_id, buffer)
    }

    pub fn delete_range(&self, table_id: u32, col_id: u32, ptr: u32) -> i32 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);
        self.instance_env.delete_range(table_id, col_id, buffer)
    }

    pub fn create_table(&self, ptr: u32) -> u32 {
        let buffer = Self::read_output_bytes(self.memory.get_ref().expect("Initialized memory"), ptr);
        self.instance_env.create_table(buffer)
    }

    pub fn iter(&self, table_id: u32) -> u64 {
        let bytes = self.instance_env.iter(table_id);
        let memory = self.memory.get_ref().expect("Initialized memory");

        let alloc_func = self.alloc.get_ref().expect("Intialized alloc function");
        let ptr = alloc_func.call(bytes.len() as u32).unwrap();

        let memory_size = memory.size().bytes().0;
        let end = (ptr.offset() as usize).checked_add(bytes.len()).unwrap();
        if end > memory_size {
            panic!("Ran off end of memory!");
        }

        unsafe {
            let write_ptr = memory.data_ptr().add(ptr.offset() as usize);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), write_ptr, bytes.len());
        }

        let mut data = ptr.offset() as u64;
        data = data << 32 | bytes.len() as u64;
        return data;
    }
}
