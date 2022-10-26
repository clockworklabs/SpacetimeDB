use crate::error::NodesError;
use crate::host::instance_env::InstanceEnv;
use wasmer::{FunctionEnvMut, WasmPtr};

use super::Mem;

pub struct WasmInstanceEnv {
    pub instance_env: InstanceEnv,
    pub mem: Option<Mem>,
}

fn cvt_count(x: Result<u32, NodesError>) -> u32 {
    match x {
        Ok(count) => count,
        Err(_) => u32::MAX,
    }
}
fn cvt(x: Result<(), NodesError>) -> u8 {
    match x {
        Ok(()) => 1,
        Err(_) => 0,
    }
}

/// Wraps an InstanceEnv with the magic necessary to push and pull bytes from webassembly
/// memory.
impl WasmInstanceEnv {
    pub fn mem(&self) -> &Mem {
        self.mem.as_ref().expect("Initialized memory")
    }

    fn read_output_bytes(caller: &FunctionEnvMut<'_, Self>, ptr: WasmPtr<u8>, len: u32) -> Vec<u8> {
        let mem = caller.data().mem();
        mem.read_output_bytes(&caller, ptr, len).expect("invalid ptr")
    }

    pub fn console_log(caller: FunctionEnvMut<'_, Self>, level: u8, ptr: WasmPtr<u8>, len: u32) {
        let buffer = Self::read_output_bytes(&caller, ptr, len);
        let s = String::from_utf8_lossy(&buffer);
        caller.data().instance_env.console_log(level, &s);
    }

    pub fn insert(caller: FunctionEnvMut<'_, Self>, table_id: u32, ptr: WasmPtr<u8>, len: u32) -> u8 {
        let buffer = Self::read_output_bytes(&caller, ptr, len).into();
        cvt(caller.data().instance_env.insert(table_id, buffer))
    }

    pub fn delete_pk(caller: FunctionEnvMut<'_, Self>, table_id: u32, ptr: WasmPtr<u8>, len: u32) -> u8 {
        let buffer = Self::read_output_bytes(&caller, ptr, len).into();
        cvt(caller.data().instance_env.delete_pk(table_id, buffer))
    }

    pub fn delete_value(caller: FunctionEnvMut<'_, Self>, table_id: u32, ptr: WasmPtr<u8>, len: u32) -> u8 {
        let buffer = Self::read_output_bytes(&caller, ptr, len).into();
        cvt(caller.data().instance_env.delete_value(table_id, buffer))
    }

    pub fn delete_eq(caller: FunctionEnvMut<'_, Self>, table_id: u32, col_id: u32, ptr: WasmPtr<u8>, len: u32) -> u32 {
        let buffer = Self::read_output_bytes(&caller, ptr, len).into();
        cvt_count(caller.data().instance_env.delete_eq(table_id, col_id, buffer))
    }

    pub fn delete_range(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        col_id: u32,
        ptr: WasmPtr<u8>,
        len: u32,
    ) -> u32 {
        let buffer = Self::read_output_bytes(&caller, ptr, len).into();
        cvt_count(caller.data().instance_env.delete_range(table_id, col_id, buffer))
    }

    pub fn create_table(caller: FunctionEnvMut<'_, Self>, ptr: WasmPtr<u8>, len: u32) -> u32 {
        let buffer = Self::read_output_bytes(&caller, ptr, len).into();
        caller.data().instance_env.create_table(buffer)
    }

    pub fn get_table_id(caller: FunctionEnvMut<'_, Self>, ptr: WasmPtr<u8>, len: u32) -> u32 {
        let buffer = Self::read_output_bytes(&caller, ptr, len).into();
        match caller.data().instance_env.get_table_id(buffer) {
            Some(value) => value,
            None => u32::MAX,
        }
    }

    fn alloc_return_ptr(mut caller: FunctionEnvMut<'_, Self>, data: &[u8]) -> u64 {
        let mem = caller.data().mem().clone();

        let (ptr, data_len) = mem.alloc_slice(&mut caller, data).unwrap();

        (ptr as u64) << 32 | data_len as u64
    }

    pub fn iter(caller: FunctionEnvMut<'_, Self>, table_id: u32) -> u64 {
        let bytes = caller.data().instance_env.iter(table_id);
        Self::alloc_return_ptr(caller, &bytes)
    }
}
