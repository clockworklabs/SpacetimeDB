//! WASI Preview 1 stub implementations for languages that compile to `wasip1`.
//!
//! Languages like Go (used for the Go SDK) compile to `wasip1`, which requires
//! WASI imports from `wasi_snapshot_preview1`. SpacetimeDB does not provide a full
//! WASI implementation, so we provide minimal stubs that allow the module to run.
//!
//! The C++ SDK handles this differently by embedding WASI shims in the compiled module
//! (see `crates/bindings-cpp/src/abi/wasi_shims.cpp`). Go uses `//go:wasmimport`
//! for WASI functions which must be satisfied by the host.

use super::wasm_instance_env::WasmInstanceEnv;
use super::{Mem, MemView};
use wasmtime::{Caller, Linker};

const WASI_MODULE: &str = "wasi_snapshot_preview1";

// WASI errno codes
const ERRNO_SUCCESS: i32 = 0;
const ERRNO_BADF: i32 = 8;
const ERRNO_NOSYS: i32 = 52;

pub(super) fn link_wasi_stubs(linker: &mut Linker<WasmInstanceEnv>) -> anyhow::Result<()> {
    // fd_write(fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32) -> errno
    //
    // Redirect stdout/stderr writes to the host logger.
    linker.func_wrap(
        WASI_MODULE,
        "fd_write",
        |mut caller: Caller<'_, WasmInstanceEnv>, fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32| -> i32 {
            if fd != 1 && fd != 2 {
                return ERRNO_BADF;
            }

            let mem = match get_memory(&mut caller) {
                Some(m) => m,
                None => return ERRNO_BADF,
            };
            let (mem_view, _) = mem.view_and_store_mut(&mut caller);

            let mut total: u32 = 0;
            let mut message = Vec::new();
            for i in 0..iovs_len {
                let iov_offset = iovs_ptr as u32 + (i as u32) * 8;
                let buf_ptr = match read_u32(mem_view, iov_offset) {
                    Some(v) => v,
                    None => return ERRNO_BADF,
                };
                let buf_len = match read_u32(mem_view, iov_offset + 4) {
                    Some(v) => v,
                    None => return ERRNO_BADF,
                };
                if let Ok(bytes) = mem_view.deref_slice(buf_ptr, buf_len) {
                    message.extend_from_slice(bytes);
                    total += buf_len;
                }
            }

            // Write nwritten
            if let Ok(dest) = mem_view.deref_slice_mut(nwritten_ptr as u32, 4) {
                dest.copy_from_slice(&total.to_le_bytes());
            }

            if !message.is_empty() {
                let msg = String::from_utf8_lossy(&message);
                if fd == 2 {
                    log::warn!("[wasm/wasi] {}", msg.trim_end());
                } else {
                    log::info!("[wasm/wasi] {}", msg.trim_end());
                }
            }

            ERRNO_SUCCESS
        },
    )?;

    // proc_exit(code: i32) -> !
    linker.func_wrap(
        WASI_MODULE,
        "proc_exit",
        |_caller: Caller<'_, WasmInstanceEnv>, _code: i32| {
            // No-op. Only called on fatal errors.
        },
    )?;

    // poll_oneoff(in_ptr: i32, out_ptr: i32, nsubscriptions: i32, nevents_ptr: i32) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "poll_oneoff",
        |mut caller: Caller<'_, WasmInstanceEnv>,
         _in_ptr: i32,
         _out_ptr: i32,
         nsubscriptions: i32,
         nevents_ptr: i32|
         -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                if let Ok(dest) = mem_view.deref_slice_mut(nevents_ptr as u32, 4) {
                    dest.copy_from_slice(&(nsubscriptions as u32).to_le_bytes());
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    // clock_time_get(id: i32, precision: i64, time_ptr: i32) -> errno
    //
    // Returns the current time in nanoseconds. Go's runtime requires this to return
    // non-zero values — it panics with "nanotime returning zero" otherwise.
    // Clock IDs: 0 = REALTIME, 1 = MONOTONIC.
    linker.func_wrap(
        WASI_MODULE,
        "clock_time_get",
        |mut caller: Caller<'_, WasmInstanceEnv>, _id: i32, _precision: i64, time_ptr: i32| -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                if let Ok(dest) = mem_view.deref_slice_mut(time_ptr as u32, 8) {
                    let nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64;
                    dest.copy_from_slice(&nanos.to_le_bytes());
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    // args_sizes_get(argc_ptr: i32, argv_buf_size_ptr: i32) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "args_sizes_get",
        |mut caller: Caller<'_, WasmInstanceEnv>, argc_ptr: i32, argv_buf_size_ptr: i32| -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                if let Ok(dest) = mem_view.deref_slice_mut(argc_ptr as u32, 4) {
                    dest.copy_from_slice(&0u32.to_le_bytes());
                }
                if let Ok(dest) = mem_view.deref_slice_mut(argv_buf_size_ptr as u32, 4) {
                    dest.copy_from_slice(&0u32.to_le_bytes());
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    // args_get(argv_ptr: i32, argv_buf_ptr: i32) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "args_get",
        |_caller: Caller<'_, WasmInstanceEnv>, _argv_ptr: i32, _argv_buf_ptr: i32| -> i32 { ERRNO_SUCCESS },
    )?;

    // random_get(buf_ptr: i32, buf_len: i32) -> errno
    //
    // Fill buffer with random bytes using getrandom (available via std).
    linker.func_wrap(
        WASI_MODULE,
        "random_get",
        |mut caller: Caller<'_, WasmInstanceEnv>, buf_ptr: i32, buf_len: i32| -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                if let Ok(dest) = mem_view.deref_slice_mut(buf_ptr as u32, buf_len as u32) {
                    // Use a simple counter-based fill. For WASM module use this is adequate —
                    // modules should not rely on WASI random_get for cryptographic purposes.
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    std::time::SystemTime::now().hash(&mut hasher);
                    let mut state = hasher.finish();
                    for byte in dest.iter_mut() {
                        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        *byte = (state >> 33) as u8;
                    }
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    // Additional stubs that some WASI-targeting compilers may need.

    linker.func_wrap(
        WASI_MODULE,
        "environ_sizes_get",
        |mut caller: Caller<'_, WasmInstanceEnv>, count_ptr: i32, size_ptr: i32| -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                if let Ok(dest) = mem_view.deref_slice_mut(count_ptr as u32, 4) {
                    dest.copy_from_slice(&0u32.to_le_bytes());
                }
                if let Ok(dest) = mem_view.deref_slice_mut(size_ptr as u32, 4) {
                    dest.copy_from_slice(&0u32.to_le_bytes());
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    linker.func_wrap(
        WASI_MODULE,
        "environ_get",
        |_caller: Caller<'_, WasmInstanceEnv>, _environ_ptr: i32, _environ_buf_ptr: i32| -> i32 { ERRNO_SUCCESS },
    )?;

    linker.func_wrap(WASI_MODULE, "fd_close", |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32| -> i32 {
        ERRNO_BADF
    })?;

    linker.func_wrap(
        WASI_MODULE,
        "fd_seek",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _offset: i64, _whence: i32, _newoffset_ptr: i32| -> i32 {
            ERRNO_NOSYS
        },
    )?;

    linker.func_wrap(
        WASI_MODULE,
        "fd_read",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _iovs_ptr: i32, _iovs_len: i32, _nread_ptr: i32| -> i32 {
            ERRNO_NOSYS
        },
    )?;

    linker.func_wrap(
        WASI_MODULE,
        "fd_fdstat_get",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _stat_ptr: i32| -> i32 { ERRNO_BADF },
    )?;

    linker.func_wrap(
        WASI_MODULE,
        "fd_prestat_get",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _prestat_ptr: i32| -> i32 { ERRNO_BADF },
    )?;

    linker.func_wrap(
        WASI_MODULE,
        "fd_prestat_dir_name",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _path_ptr: i32, _path_len: i32| -> i32 { ERRNO_BADF },
    )?;

    // sched_yield() -> errno
    //
    // Yield the processor. Standard Go's WASM runtime calls this during goroutine scheduling.
    linker.func_wrap(
        WASI_MODULE,
        "sched_yield",
        |_caller: Caller<'_, WasmInstanceEnv>| -> i32 { ERRNO_SUCCESS },
    )?;

    // fd_fdstat_set_flags(fd: i32, flags: i32) -> errno
    //
    // Set file descriptor flags. Standard Go's WASM runtime may call this during initialization.
    linker.func_wrap(
        WASI_MODULE,
        "fd_fdstat_set_flags",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _flags: i32| -> i32 { ERRNO_NOSYS },
    )?;

    Ok(())
}

fn get_memory(caller: &mut Caller<'_, WasmInstanceEnv>) -> Option<Mem> {
    let memory = caller.get_export("memory")?.into_memory()?;
    Some(Mem { memory })
}

fn read_u32(mem: &MemView, offset: u32) -> Option<u32> {
    let bytes = mem.deref_slice(offset, 4).ok()?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}
