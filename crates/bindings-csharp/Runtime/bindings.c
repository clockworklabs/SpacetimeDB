#include <assert.h>
// #include <mono/metadata/appdomain.h>
// #include <mono/metadata/object.h>
#include <stdint.h>
#include <unistd.h>

#include "driver.h"

#define OPAQUE_TYPEDEF(name, T) \
  typedef struct name {         \
    T inner;                    \
  } name

OPAQUE_TYPEDEF(Status, uint16_t);
OPAQUE_TYPEDEF(TableId, uint32_t);
OPAQUE_TYPEDEF(ColId, uint32_t);
OPAQUE_TYPEDEF(IndexType, uint8_t);
OPAQUE_TYPEDEF(LogLevel, uint8_t);
OPAQUE_TYPEDEF(ScheduleToken, uint64_t);
OPAQUE_TYPEDEF(Buffer, uint32_t);
OPAQUE_TYPEDEF(BufferIter, uint32_t);

#define CSTR(s) (uint8_t*)s, sizeof(s) - 1

#define IMPORT(ret, name, params, args)                             \
  __attribute__((import_module("spacetime_7.0"),                    \
                 import_name(#name))) extern ret name##_imp params; \
  ret name params { return name##_imp args; }

IMPORT(void, _console_log,
       (LogLevel level, const uint8_t* target, uint32_t target_len,
        const uint8_t* filename, uint32_t filename_len, uint32_t line_number,
        const uint8_t* message, uint32_t message_len),
       (level, target, target_len, filename, filename_len, line_number, message,
        message_len));

IMPORT(Status, _get_table_id,
       (const uint8_t* name, uint32_t name_len, TableId* id),
       (name, name_len, id));
IMPORT(Status, _create_index,
       (const uint8_t* index_name, uint32_t index_name_len, TableId table_id,
        const ColId* col_ids, uint32_t col_ids_len, IndexType type),
       (index_name, index_name_len, table_id, col_ids, col_ids_len, type));
IMPORT(Status, _iter_by_col_eq,
       (TableId table_id, ColId col_id, const uint8_t* value,
        uint32_t value_len, BufferIter* iter),
       (table_id, col_id, value, value_len, iter));
IMPORT(Status, _insert, (TableId table_id, const uint8_t* row, uint32_t len),
       (table_id, row, len));
IMPORT(Status, _delete_by_col_eq,
       (TableId table_id, ColId col_id, const uint8_t* value,
        uint32_t value_len, uint32_t* num_deleted),
       (table_id, col_id, value, value_len, num_deleted));
IMPORT(Status, _delete_by_rel,
       (TableId table_id, const uint8_t* relation, uint32_t relation_len,
        uint32_t* num_deleted),
       (table_id, relation, relation_len, num_deleted));
IMPORT(Status, _iter_start, (TableId table_id, BufferIter* iter),
       (table_id, iter));
IMPORT(Status, _iter_start_filtered,
       (TableId table_id, const uint8_t* filter, uint32_t filter_len,
        BufferIter* iter),
       (table_id, filter, filter_len, iter));
IMPORT(Status, _iter_next, (BufferIter iter, Buffer* row), (iter, row));
IMPORT(Status, _iter_drop, (BufferIter iter), (iter));
IMPORT(void, _schedule_reducer,
       (const uint8_t* name, uint32_t name_len, const uint8_t* args,
        uint32_t args_len, uint64_t timestamp, ScheduleToken* token),
       (name, name_len, args, args_len, timestamp, token));
IMPORT(void, _cancel_reducer, (ScheduleToken token), (token));
IMPORT(uint32_t, _buffer_len, (Buffer buf), (buf));
IMPORT(void, _buffer_consume, (Buffer buf, uint8_t* dst, uint32_t dst_len),
       (buf, dst, dst_len));
IMPORT(Buffer, _buffer_alloc, (const uint8_t* data, uint32_t len), (data, len));

static MonoClass* ffi_class;

#define CEXPORT(name) __attribute__((export_name(#name))) name

#define PREINIT(priority, name) void CEXPORT(__preinit__##priority##_##name)()

PREINIT(10, startup) {
  // mono_wasm_load_runtime("", 0);
  // ^ not enough because it doesn't reach to assembly with Main function
  // so module descriptor remains unpopulated. Invoke actual _start instead.
  extern void _start();
  _start();

  ffi_class = mono_wasm_assembly_find_class(
      mono_wasm_assembly_load("SpacetimeDB.Runtime.dll"), "SpacetimeDB.Module",
      "FFI");
  assert(ffi_class && "FFI class not found");
}

#define EXPORT(ret, name, params, args...)                                    \
  static MonoMethod* ffi_method_##name;                                       \
  PREINIT(20, find_##name) {                                                  \
    ffi_method_##name = mono_wasm_assembly_find_method(ffi_class, #name, -1); \
    assert(ffi_method_##name && "FFI method not found");                      \
  }                                                                           \
  ret CEXPORT(name) params {                                                  \
    MonoObject* res;                                                          \
    mono_wasm_invoke_method_ref(ffi_method_##name, NULL, (void*[]){args},     \
                                NULL, &res);                                  \
    return *(ret*)mono_object_unbox(res);                                     \
  }

EXPORT(Buffer, __describe_module__, ());

EXPORT(Buffer, __call_reducer__,
       (uint32_t id, Buffer caller_identity, Buffer caller_address,
        uint64_t timestamp, Buffer args),
       &id, &caller_identity, &caller_address, &timestamp, &args);

// Shims to avoid dependency on WASI in the generated Wasm file.

#include <stdlib.h>
#include <wasi/api.h>

// Ignore warnings about anonymous parameters, this is to avoid having
// to write `int arg0`, `int arg1`, etc. for every function.
#pragma clang diagnostic ignored "-Wc2x-extensions"

// Based on
// https://github.com/WebAssembly/wasi-libc/blob/main/libc-bottom-half/sources/__wasilibc_real.c,

#define WASI_NAME(name) __imported_wasi_snapshot_preview1_##name

// Shim for WASI calls that always unconditionaly succeeds.
// This is suitable for most (but not all) WASI functions used by .NET.
#define WASI_SHIM(name, params) \
  int32_t WASI_NAME(name) params { return 0; }

WASI_SHIM(environ_get, (int32_t, int32_t));
WASI_SHIM(environ_sizes_get, (int32_t, int32_t));
WASI_SHIM(clock_time_get, (int32_t, int64_t, int32_t));
WASI_SHIM(fd_advise, (int32_t, int64_t, int64_t, int32_t));
WASI_SHIM(fd_allocate, (int32_t, int64_t, int64_t));
WASI_SHIM(fd_close, (int32_t));
WASI_SHIM(fd_datasync, (int32_t));
WASI_SHIM(fd_fdstat_get, (int32_t, int32_t));
WASI_SHIM(fd_fdstat_set_flags, (int32_t, int32_t));
WASI_SHIM(fd_fdstat_set_rights, (int32_t, int64_t, int64_t));
WASI_SHIM(fd_filestat_get, (int32_t, int32_t));
WASI_SHIM(fd_filestat_set_size, (int32_t, int64_t));
WASI_SHIM(fd_filestat_set_times, (int32_t, int64_t, int64_t, int32_t));
WASI_SHIM(fd_pread, (int32_t, int32_t, int32_t, int64_t, int32_t));
WASI_SHIM(fd_prestat_dir_name, (int32_t, int32_t, int32_t));
WASI_SHIM(fd_pwrite, (int32_t, int32_t, int32_t, int64_t, int32_t));
WASI_SHIM(fd_read, (int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(fd_readdir, (int32_t, int32_t, int32_t, int64_t, int32_t));
WASI_SHIM(fd_renumber, (int32_t, int32_t));
WASI_SHIM(fd_seek, (int32_t, int64_t, int32_t, int32_t));
WASI_SHIM(fd_sync, (int32_t));
WASI_SHIM(fd_tell, (int32_t, int32_t));
WASI_SHIM(path_create_directory, (int32_t, int32_t, int32_t));
WASI_SHIM(path_filestat_get, (int32_t, int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(path_filestat_set_times,
          (int32_t, int32_t, int32_t, int32_t, int64_t, int64_t, int32_t));
WASI_SHIM(path_link,
          (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(path_open, (int32_t, int32_t, int32_t, int32_t, int32_t, int64_t,
                      int64_t, int32_t, int32_t));
WASI_SHIM(path_readlink,
          (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(path_remove_directory, (int32_t, int32_t, int32_t));
WASI_SHIM(path_rename, (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(path_symlink, (int32_t, int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(path_unlink_file, (int32_t, int32_t, int32_t));
WASI_SHIM(poll_oneoff, (int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(sched_yield, ());
WASI_SHIM(random_get, (int32_t, int32_t));
WASI_SHIM(sock_accept, (int32_t, int32_t, int32_t));
WASI_SHIM(sock_recv, (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(sock_send, (int32_t, int32_t, int32_t, int32_t, int32_t));
WASI_SHIM(sock_shutdown, (int32_t, int32_t));

// Mono retrieves executable name via argv[0], so we need to shim it with
// some dummy name instead of returning an empty argv[] array to avoid
// assertion failures.
const char executable_name[] = "stdb.wasm";

int32_t WASI_NAME(args_sizes_get)(__wasi_size_t* argc,
                                  __wasi_size_t* argv_buf_size) {
  *argc = 1;
  *argv_buf_size = sizeof(executable_name);
  return 0;
}

int32_t WASI_NAME(args_get)(uint8_t** argv, uint8_t* argv_buf) {
  argv[0] = argv_buf;
  __builtin_memcpy(argv_buf, executable_name, sizeof(executable_name));
  return 0;
}

// Clock resolution should be non-zero.
int32_t WASI_NAME(clock_res_get)(int32_t, uint64_t* timestamp) {
  *timestamp = 1;
  return 0;
}

// For `fd_write`, we need to at least collect and report sum of sizes.
// If we report size 0, the caller will assume that the write failed and will
// try again, which will result in an infinite loop.
int32_t WASI_NAME(fd_write)(__wasi_fd_t fd, const __wasi_ciovec_t* iovs,
                            size_t iovs_len, __wasi_size_t* retptr0) {
  for (size_t i = 0; i < iovs_len; i++) {
    // Note: this will produce ugly broken output, but there's not much we can
    // do about it until we have proper line-buffered WASI writer in the core.
    // It's better than nothing though.
    _console_log_imp((LogLevel){fd == STDERR_FILENO ? /*WARN*/ 1 : /*INFO*/ 2},
                     CSTR("wasi"), CSTR(__FILE__), __LINE__, iovs[i].buf,
                     iovs[i].buf_len);
    *retptr0 += iovs[i].buf_len;
  }
  return 0;
}

// BADF indicates end of iteration for preopens; we must return it instead of
// "success" to prevent infinite loop.
int32_t WASI_NAME(fd_prestat_get)(int32_t, int32_t) {
  return __WASI_ERRNO_BADF;
}

// Actually exit runtime on `proc_exit`.
_Noreturn void WASI_NAME(proc_exit)(int32_t code) { exit(code); }

// There is another rogue import of sock_accept somewhere in .NET that doesn't
// match the scheme above.
// Maybe this one?
// https://github.com/dotnet/runtime/blob/085ddb7f9b26f01ae1b6842db7eacb6b4042e031/src/mono/mono/component/mini-wasi-debugger.c#L12-L14

int32_t sock_accept(int32_t, int32_t, int32_t) { return 0; }
