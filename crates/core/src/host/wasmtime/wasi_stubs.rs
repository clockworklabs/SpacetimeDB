//! WASI Preview 1 stub implementations for languages that compile to `wasip1`.
//!
//! Languages like Go, C#, and C++ compile to `wasip1`, which requires WASI imports
//! from `wasi_snapshot_preview1`. SpacetimeDB does not provide a full WASI
//! implementation, so we provide minimal stubs that allow the module to run.
//!
//! These stubs are a superset of everything needed by Go, C#/.NET (Mono), and C++
//! (Emscripten). When a module embeds its own WASI shims (as the C# and C++ SDKs
//! currently do), the module-side definitions take precedence over host stubs.

use super::wasm_instance_env::WasmInstanceEnv;
use super::{Mem, MemView};
use wasmtime::{Caller, Linker};

const WASI_MODULE: &str = "wasi_snapshot_preview1";
const ENV_MODULE: &str = "env";

// WASI errno codes
const ERRNO_SUCCESS: i32 = 0;
const ERRNO_BADF: i32 = 8;
const ERRNO_NOSYS: i32 = 52;

// The dummy executable name provided via args_get/args_sizes_get.
// Mono runtime crashes without argv[0].
const EXECUTABLE_NAME: &[u8] = b"stdb.wasm\0";

pub(super) fn link_wasi_stubs(linker: &mut Linker<WasmInstanceEnv>) -> anyhow::Result<()> {
    // =========================================================================
    // fd_write — redirect stdout/stderr to host logger
    // =========================================================================

    // fd_write(fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32) -> errno
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

    // =========================================================================
    // proc_exit — trap to cleanly abort execution
    // =========================================================================

    // proc_exit(code: i32) -> !
    //
    // Trap to cleanly abort WASM execution. This is better than a no-op because
    // it prevents the module from continuing in an undefined state after exit.
    linker.func_wrap(
        WASI_MODULE,
        "proc_exit",
        |_caller: Caller<'_, WasmInstanceEnv>, code: i32| -> anyhow::Result<()> {
            anyhow::bail!("proc_exit called with code {code}")
        },
    )?;

    // =========================================================================
    // poll_oneoff — pretend all subscriptions fired
    // =========================================================================

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

    // =========================================================================
    // Clock functions
    // =========================================================================

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

    // clock_res_get(id: i32, resolution_ptr: i32) -> errno
    //
    // Returns the resolution (precision) of a clock. Both C# and C++ need this.
    // We return 1 nanosecond as the resolution.
    linker.func_wrap(
        WASI_MODULE,
        "clock_res_get",
        |mut caller: Caller<'_, WasmInstanceEnv>, _id: i32, resolution_ptr: i32| -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                if let Ok(dest) = mem_view.deref_slice_mut(resolution_ptr as u32, 8) {
                    dest.copy_from_slice(&1u64.to_le_bytes());
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    // =========================================================================
    // args — provide argv[0] = "stdb.wasm" (Mono crashes without it)
    // =========================================================================

    // args_sizes_get(argc_ptr: i32, argv_buf_size_ptr: i32) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "args_sizes_get",
        |mut caller: Caller<'_, WasmInstanceEnv>, argc_ptr: i32, argv_buf_size_ptr: i32| -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                // argc = 1
                if let Ok(dest) = mem_view.deref_slice_mut(argc_ptr as u32, 4) {
                    dest.copy_from_slice(&1u32.to_le_bytes());
                }
                // argv_buf_size = length of "stdb.wasm\0"
                if let Ok(dest) = mem_view.deref_slice_mut(argv_buf_size_ptr as u32, 4) {
                    dest.copy_from_slice(&(EXECUTABLE_NAME.len() as u32).to_le_bytes());
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    // args_get(argv_ptr: i32, argv_buf_ptr: i32) -> errno
    //
    // Write a pointer to the buffer in argv[0], then copy "stdb.wasm\0" into argv_buf.
    linker.func_wrap(
        WASI_MODULE,
        "args_get",
        |mut caller: Caller<'_, WasmInstanceEnv>, argv_ptr: i32, argv_buf_ptr: i32| -> i32 {
            if let Some(mem) = get_memory(&mut caller) {
                let (mem_view, _) = mem.view_and_store_mut(&mut caller);
                // argv[0] = argv_buf_ptr
                if let Ok(dest) = mem_view.deref_slice_mut(argv_ptr as u32, 4) {
                    dest.copy_from_slice(&(argv_buf_ptr as u32).to_le_bytes());
                }
                // Copy "stdb.wasm\0" into argv_buf
                if let Ok(dest) = mem_view.deref_slice_mut(argv_buf_ptr as u32, EXECUTABLE_NAME.len() as u32) {
                    dest.copy_from_slice(EXECUTABLE_NAME);
                }
            }
            ERRNO_SUCCESS
        },
    )?;

    // =========================================================================
    // random_get — fill buffer with pseudo-random bytes
    // =========================================================================

    // random_get(buf_ptr: i32, buf_len: i32) -> errno
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

    // =========================================================================
    // Environment stubs
    // =========================================================================

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

    // =========================================================================
    // Scheduler
    // =========================================================================

    // sched_yield() -> errno
    //
    // Yield the processor. Standard Go's WASM runtime calls this during goroutine scheduling.
    linker.func_wrap(
        WASI_MODULE,
        "sched_yield",
        |_caller: Caller<'_, WasmInstanceEnv>| -> i32 { ERRNO_SUCCESS },
    )?;

    // =========================================================================
    // File descriptor stubs
    // =========================================================================

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

    // fd_fdstat_set_flags(fd: i32, flags: i32) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_fdstat_set_flags",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _flags: i32| -> i32 { ERRNO_NOSYS },
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

    // --- New fd stubs needed by C#/C++ (all return ERRNO_NOSYS) ---

    // fd_advise(fd, offset: i64, len: i64, advice) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_advise",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _offset: i64, _len: i64, _advice: i32| -> i32 {
            ERRNO_NOSYS
        },
    )?;

    // fd_allocate(fd, offset: i64, len: i64) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_allocate",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _offset: i64, _len: i64| -> i32 { ERRNO_NOSYS },
    )?;

    // fd_datasync(fd) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_datasync",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // fd_fdstat_set_rights(fd, rights_base: i64, rights_inheriting: i64) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_fdstat_set_rights",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _rights_base: i64, _rights_inheriting: i64| -> i32 {
            ERRNO_NOSYS
        },
    )?;

    // fd_filestat_get(fd, stat_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_filestat_get",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _stat_ptr: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // fd_filestat_set_size(fd, size: i64) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_filestat_set_size",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _size: i64| -> i32 { ERRNO_NOSYS },
    )?;

    // fd_filestat_set_times(fd, atim: i64, mtim: i64, fst_flags) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_filestat_set_times",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _atim: i64, _mtim: i64, _fst_flags: i32| -> i32 {
            ERRNO_NOSYS
        },
    )?;

    // fd_pread(fd, iovs_ptr, iovs_len, offset: i64, nread_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_pread",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _iovs_ptr: i32,
         _iovs_len: i32,
         _offset: i64,
         _nread_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // fd_pwrite(fd, iovs_ptr, iovs_len, offset: i64, nwritten_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_pwrite",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _iovs_ptr: i32,
         _iovs_len: i32,
         _offset: i64,
         _nwritten_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // fd_readdir(fd, buf_ptr, buf_len, cookie: i64, bufused_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_readdir",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _buf_ptr: i32,
         _buf_len: i32,
         _cookie: i64,
         _bufused_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // fd_renumber(from_fd, to_fd) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_renumber",
        |_caller: Caller<'_, WasmInstanceEnv>, _from_fd: i32, _to_fd: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // fd_sync(fd) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_sync",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // fd_tell(fd, offset_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "fd_tell",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _offset_ptr: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // =========================================================================
    // Path stubs (all return ERRNO_NOSYS)
    // =========================================================================

    // path_create_directory(fd, path_ptr, path_len) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_create_directory",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _path_ptr: i32, _path_len: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // path_filestat_get(fd, flags, path_ptr, path_len, stat_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_filestat_get",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _flags: i32,
         _path_ptr: i32,
         _path_len: i32,
         _stat_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // path_filestat_set_times(fd, flags, path_ptr, path_len, atim: i64, mtim: i64, fst_flags) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_filestat_set_times",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _flags: i32,
         _path_ptr: i32,
         _path_len: i32,
         _atim: i64,
         _mtim: i64,
         _fst_flags: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // path_link(old_fd, old_flags, old_path_ptr, old_path_len, new_fd, new_path_ptr, new_path_len) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_link",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _old_fd: i32,
         _old_flags: i32,
         _old_path_ptr: i32,
         _old_path_len: i32,
         _new_fd: i32,
         _new_path_ptr: i32,
         _new_path_len: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // path_open(fd, dirflags, path_ptr, path_len, oflags, rights_base: i64, rights_inheriting: i64, fdflags, result_fd_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_open",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _dirflags: i32,
         _path_ptr: i32,
         _path_len: i32,
         _oflags: i32,
         _rights_base: i64,
         _rights_inheriting: i64,
         _fdflags: i32,
         _result_fd_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // path_readlink(fd, path_ptr, path_len, buf_ptr, buf_len, bufused_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_readlink",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _path_ptr: i32,
         _path_len: i32,
         _buf_ptr: i32,
         _buf_len: i32,
         _bufused_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // path_remove_directory(fd, path_ptr, path_len) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_remove_directory",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _path_ptr: i32, _path_len: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // path_rename(old_fd, old_path_ptr, old_path_len, new_fd, new_path_ptr, new_path_len) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_rename",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _old_fd: i32,
         _old_path_ptr: i32,
         _old_path_len: i32,
         _new_fd: i32,
         _new_path_ptr: i32,
         _new_path_len: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // path_symlink(old_path_ptr, old_path_len, fd, new_path_ptr, new_path_len) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_symlink",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _old_path_ptr: i32,
         _old_path_len: i32,
         _fd: i32,
         _new_path_ptr: i32,
         _new_path_len: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // path_unlink_file(fd, path_ptr, path_len) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "path_unlink_file",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _path_ptr: i32, _path_len: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // =========================================================================
    // Socket stubs (all return ERRNO_NOSYS)
    // =========================================================================

    // sock_accept(fd, flags, result_fd_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "sock_accept",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _flags: i32, _result_fd_ptr: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // sock_recv(fd, ri_data_ptr, ri_data_len, ri_flags, ro_datalen_ptr, ro_flags_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "sock_recv",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _ri_data_ptr: i32,
         _ri_data_len: i32,
         _ri_flags: i32,
         _ro_datalen_ptr: i32,
         _ro_flags_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // sock_send(fd, si_data_ptr, si_data_len, si_flags, so_datalen_ptr) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "sock_send",
        |_caller: Caller<'_, WasmInstanceEnv>,
         _fd: i32,
         _si_data_ptr: i32,
         _si_data_len: i32,
         _si_flags: i32,
         _so_datalen_ptr: i32|
         -> i32 { ERRNO_NOSYS },
    )?;

    // sock_shutdown(fd, how) -> errno
    linker.func_wrap(
        WASI_MODULE,
        "sock_shutdown",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _how: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // =========================================================================
    // Non-WASI stubs under the "env" module
    // =========================================================================

    // sock_accept — rogue import from .NET/Mono that uses the "env" module
    // instead of "wasi_snapshot_preview1".
    // See: https://github.com/dotnet/runtime/blob/085ddb7f9b26f01ae1b6842db7eacb6b4042e031/src/mono/mono/component/mini-wasi-debugger.c#L12-L14
    linker.func_wrap(
        ENV_MODULE,
        "sock_accept",
        |_caller: Caller<'_, WasmInstanceEnv>, _fd: i32, _flags: i32, _result_fd_ptr: i32| -> i32 { ERRNO_NOSYS },
    )?;

    // emscripten_notify_memory_growth — Emscripten runtime callback invoked
    // when linear memory grows. C++ modules built with Emscripten import this.
    linker.func_wrap(
        ENV_MODULE,
        "emscripten_notify_memory_growth",
        |_caller: Caller<'_, WasmInstanceEnv>, _memory_index: i32| {
            // No-op — memory growth is handled by the Wasmtime runtime.
        },
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
