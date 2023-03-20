#![allow(clippy::too_many_arguments)]

use crate::database_logger::{BacktraceFrame, BacktraceProvider, ModuleBacktrace, Record};
use crate::error::NodesError;
use crate::host::timestamp::Timestamp;
use crate::host::wasm_common::{err_to_errno, AbiRuntimeError, BufferIdx, BufferIterIdx, BufferIters, Buffers};
use bytes::Bytes;
use itertools::Itertools;
use wasmer::{FunctionEnvMut, MemoryAccessError, RuntimeError, ValueType, WasmPtr};

use crate::host::instance_env::InstanceEnv;

use super::{Mem, WasmError};

pub(super) struct WasmInstanceEnv {
    pub instance_env: InstanceEnv,
    pub mem: Option<Mem>,
    pub buffers: Buffers,
    pub iters: BufferIters,
}

type WasmResult = Result<u16, RuntimeError>;

fn mem_err(err: MemoryAccessError) -> RuntimeError {
    match err {
        MemoryAccessError::HeapOutOfBounds | MemoryAccessError::Overflow => {
            RuntimeError::from_trap(wasmer_vm::Trap::lib(wasmer_vm::TrapCode::HeapAccessOutOfBounds))
        }
        _ => RuntimeError::user(err.into()),
    }
}

/// Wraps an InstanceEnv with the magic necessary to push and pull bytes from webassembly
/// memory.
impl WasmInstanceEnv {
    pub fn mem(&self) -> Mem {
        self.mem.clone().expect("Initialized memory")
    }

    fn cvt(
        mut caller: FunctionEnvMut<'_, Self>,
        func: &'static str,
        f: impl FnOnce(FunctionEnvMut<'_, Self>, &Mem) -> Result<(), WasmError>,
    ) -> Result<u16, RuntimeError> {
        let mem = caller.data().mem();
        let err = match f(caller.as_mut(), &mem) {
            Ok(()) => return Ok(0),
            Err(e) => e,
        };
        if let WasmError::Db(err) = &err {
            if let Some(errno) = err_to_errno(err) {
                log::info!("abi call to {func} returned a normal error: {err:#}");
                return Ok(errno);
            }
        }

        Err(match err {
            WasmError::Db(err) => RuntimeError::user(Box::new(AbiRuntimeError { func, err })),
            WasmError::Mem(err) => mem_err(err),
            WasmError::Wasm(err) => err,
        })
    }

    fn cvt_ret<T: ValueType>(
        caller: FunctionEnvMut<'_, Self>,
        func: &'static str,
        out: WasmPtr<T>,
        f: impl FnOnce(FunctionEnvMut<'_, Self>, &Mem) -> Result<T, WasmError>,
    ) -> Result<u16, RuntimeError> {
        Self::cvt(caller, func, |mut caller, mem| {
            f(caller.as_mut(), mem).and_then(|ret| out.write(&mem.view(&caller), ret).map_err(Into::into))
        })
    }

    pub fn schedule_reducer(
        caller: FunctionEnvMut<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        args: WasmPtr<u8>,
        args_len: u32,
        time: u64,
    ) -> Result<(), RuntimeError> {
        Self::cvt(caller, "schedule_reducer", |caller, mem| {
            let name = mem.read_bytes(&caller, name, name_len)?;
            let name = String::from_utf8(name).map_err(|_| RuntimeError::new("name must be utf8"))?;
            let args = mem.read_bytes(&caller, args, args_len)?;
            caller.data().instance_env.schedule(name, args, Timestamp(time));
            Ok(())
        })
        .map(|_| ())
    }

    pub fn console_log(
        caller: FunctionEnvMut<'_, Self>,
        level: u8,
        target: WasmPtr<u8>,
        target_len: u32,
        filename: WasmPtr<u8>,
        filename_len: u32,
        line_number: u32,
        message: WasmPtr<u8>,
        message_len: u32,
    ) {
        let mem = caller.data().mem();
        let read_str = |ptr, len| {
            mem.read_bytes(&caller, ptr, len)
                .map(crate::util::string_from_utf8_lossy_owned)
        };
        let read_opt_str = |ptr: WasmPtr<_>, len| (!ptr.is_null()).then(|| read_str(ptr, len)).transpose();
        let _ = (|| -> Result<_, MemoryAccessError> {
            let target = read_opt_str(target, target_len)?;
            let filename = read_opt_str(filename, filename_len)?;
            let message = read_str(message, message_len)?;
            let line_number = (line_number != u32::MAX).then_some(line_number);

            let record = Record {
                target: target.as_deref(),
                filename: filename.as_deref(),
                line_number,
                message: &message,
            };

            caller
                .data()
                .instance_env
                .console_log(level.into(), &record, &WasmerBacktraceProvider);
            Ok(())
        })();
    }

    pub fn insert(caller: FunctionEnvMut<'_, Self>, table_id: u32, row: WasmPtr<u8>, row_len: u32) -> WasmResult {
        Self::cvt(caller, "insert", |caller, mem| {
            let row = mem.read_bytes(&caller, row, row_len)?;
            caller.data().instance_env.insert(table_id, &row)?;
            Ok(())
        })
    }

    pub fn delete_pk(caller: FunctionEnvMut<'_, Self>, table_id: u32, pk: WasmPtr<u8>, pk_len: u32) -> WasmResult {
        Self::cvt(caller, "delete_pk", |caller, mem| {
            let pk = mem.read_bytes(&caller, pk, pk_len)?;
            caller.data().instance_env.delete_pk(table_id, &pk)?;
            Ok(())
        })
    }

    pub fn delete_value(caller: FunctionEnvMut<'_, Self>, table_id: u32, row: WasmPtr<u8>, row_len: u32) -> WasmResult {
        Self::cvt(caller, "delete_value", |caller, mem| {
            let row = mem.read_bytes(&caller, row, row_len)?;
            caller.data().instance_env.delete_value(table_id, &row)?;
            Ok(())
        })
    }

    pub fn delete_eq(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        col_id: u32,
        value: WasmPtr<u8>,
        value_len: u32,
        out: WasmPtr<u32>,
    ) -> WasmResult {
        Self::cvt_ret(caller, "delete_eq", out, |caller, mem| {
            let value = mem.read_bytes(&caller, value, value_len)?;
            let n_deleted = caller.data().instance_env.delete_eq(table_id, col_id, &value)?;
            Ok(n_deleted)
        })
    }

    pub fn delete_range(
        caller: FunctionEnvMut<'_, Self>,
        table_id: u32,
        col_id: u32,
        range_start: WasmPtr<u8>,
        range_start_len: u32,
        range_end: WasmPtr<u8>,
        range_end_len: u32,
        out: WasmPtr<u32>,
    ) -> WasmResult {
        Self::cvt_ret(caller, "delete_eq", out, |caller, mem| {
            let start = mem.read_bytes(&caller, range_start, range_start_len)?;
            let end = mem.read_bytes(&caller, range_end, range_end_len)?;
            let n_deleted = caller
                .data()
                .instance_env
                .delete_range(table_id, col_id, &start, &end)?;
            Ok(n_deleted)
        })
    }

    pub fn create_table(
        caller: FunctionEnvMut<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        schema: WasmPtr<u8>,
        schema_len: u32,
        out: WasmPtr<u32>,
    ) -> WasmResult {
        Self::cvt_ret(caller, "create_table", out, |caller, mem| {
            let name = mem.read_bytes(&caller, name, name_len)?;
            let name = std::str::from_utf8(&name).map_err(|_| RuntimeError::new("name must be utf8"))?;
            let schema = mem.read_bytes(&caller, schema, schema_len)?;
            let table_id = caller.data().instance_env.create_table(name, &schema)?;
            Ok(table_id)
        })
    }

    pub fn get_table_id(
        caller: FunctionEnvMut<'_, Self>,
        name: WasmPtr<u8>,
        name_len: u32,
        out: WasmPtr<u32>,
    ) -> WasmResult {
        Self::cvt_ret(caller, "get_table_id", out, |caller, mem| {
            let name = mem.read_bytes(&caller, name, name_len)?;
            let name = std::str::from_utf8(&name).map_err(|_| NodesError::TableNotFound)?;
            let table_id = caller.data().instance_env.get_table_id(name)?;
            Ok(table_id)
        })
    }

    pub fn iter_start(caller: FunctionEnvMut<'_, Self>, table_id: u32, out: WasmPtr<BufferIterIdx>) -> WasmResult {
        Self::cvt_ret(caller, "iter_start", out, |mut caller, _mem| {
            let iter = caller.data().instance_env.iter(table_id);
            // TODO: make it so the above iterator doesn't lock the database for its whole lifetime
            let iter = iter.map_ok(Bytes::from).collect::<Vec<_>>().into_iter();

            Ok(caller.data_mut().iters.insert(Box::new(iter)))
        })
    }

    pub fn iter_next(caller: FunctionEnvMut<'_, Self>, iter_key: u32, out: WasmPtr<BufferIdx>) -> WasmResult {
        Self::cvt_ret(caller, "iter_next", out, |mut caller, _mem| {
            let data_mut = caller.data_mut();
            let iter_key = BufferIterIdx(iter_key);

            let iter = data_mut
                .iters
                .get_mut(iter_key)
                .ok_or_else(|| RuntimeError::new("no such iterator"))?;

            match iter.next() {
                Some(Ok(buf)) => Ok(data_mut.buffers.insert(buf)),
                Some(Err(err)) => Err(err.into()),
                None => Ok(BufferIdx::INVALID),
            }
        })
    }

    pub fn iter_drop(caller: FunctionEnvMut<'_, Self>, iter_key: u32) -> WasmResult {
        Self::cvt(caller, "iter_drop", |mut caller, _mem| {
            let iter_key = BufferIterIdx(iter_key);
            drop(
                caller
                    .data_mut()
                    .iters
                    .take(iter_key)
                    .ok_or_else(|| RuntimeError::new("no such iterator"))?,
            );

            Ok(())
        })
    }

    pub fn buffer_len(caller: FunctionEnvMut<'_, Self>, buffer: u32) -> Result<u32, RuntimeError> {
        caller
            .data()
            .buffers
            .get(BufferIdx(buffer))
            .map(|b| b.len() as u32)
            .ok_or_else(|| RuntimeError::new("no such buffer"))
    }

    pub fn buffer_consume(
        mut caller: FunctionEnvMut<'_, Self>,
        buffer: u32,
        ptr: WasmPtr<u8>,
        len: u32,
    ) -> Result<(), RuntimeError> {
        let buf = caller
            .data_mut()
            .buffers
            .take(BufferIdx(buffer))
            .ok_or_else(|| RuntimeError::new("no such buffer"))?;
        ptr.slice(&caller.data().mem().view(&caller), len)
            .and_then(|slice| slice.write_slice(&buf))
            .map_err(mem_err)
    }

    pub fn buffer_alloc(
        mut caller: FunctionEnvMut<'_, Self>,
        data: WasmPtr<u8>,
        data_len: u32,
    ) -> Result<u32, RuntimeError> {
        let buf = caller
            .data()
            .mem()
            .read_bytes(&caller, data, data_len)
            .map_err(mem_err)?;
        Ok(caller.data_mut().buffers.insert(buf.into()).0)
    }
}

struct WasmerBacktraceProvider;
impl BacktraceProvider for WasmerBacktraceProvider {
    fn capture(&self) -> Box<dyn ModuleBacktrace> {
        Box::new(RuntimeError::new(""))
    }
}

impl ModuleBacktrace for RuntimeError {
    fn frames(&self) -> Vec<BacktraceFrame<'_>> {
        self.trace()
            .iter()
            .map(|f| {
                let module = f.module_name();
                BacktraceFrame {
                    module_name: (module != "<module>").then_some(module),
                    func_name: f.function_name(),
                }
            })
            .collect()
    }
}
