#include <assert.h>
#include <mono-wasi/driver.h>
#include <mono/metadata/appdomain.h>
#include <mono/metadata/object.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

// My modified version of
// https://github.com/dotnet/dotnet-wasi-sdk/blob/2dbb00c779180873d3ed985e59e431f56404d8da/src/Wasi.AspNetCore.Server.Native/native/dotnet_method.h
#define INVOKE_DOTNET_METHOD(DLL_NAME, NAMESPACE, CLASS_NAME, METHOD_NAME,    \
                             INSTANCE, ARGS...)                               \
  ({                                                                          \
    static MonoMethod* _method;                                               \
    if (!_method) {                                                           \
      _method = lookup_dotnet_method(DLL_NAME, NAMESPACE, CLASS_NAME,         \
                                     METHOD_NAME, -1);                        \
      assert(_method);                                                        \
    }                                                                         \
                                                                              \
    MonoObject* _exception;                                                   \
    MonoObject* _res = mono_wasm_invoke_method(_method, INSTANCE,             \
                                               (void*[]){ARGS}, &_exception); \
    assert(!_exception);                                                      \
    _res;                                                                     \
  })

static void check_result(uint16_t result) {
  if (result != 0) {
    // TODO: figure out how to properly throw exception.
    // `mono_raise_exception` for some reason always results in Wasm sig
    // mismatch.
    INVOKE_DOTNET_METHOD("SpacetimeDB.Runtime.dll", "SpacetimeDB", "Runtime",
                         "ThrowForResult", NULL, &result);
  }
}

typedef struct {
  uint32_t handle;
} Buffer;

typedef struct {
  uint32_t handle;
} BufferIter;

typedef struct {
  uint64_t handle;
} ScheduleToken;

#define INVALID_HANDLE ((uint32_t)-1)

typedef struct {
  char* ptr;
  size_t len;
} String;

typedef struct {
  uint8_t* ptr;
  size_t len;
} Bytes;

static String to_string(MonoString* str) {
  char* ptr = mono_string_to_utf8(str);
  size_t len = strlen(ptr);
  return (String){.ptr = ptr, .len = len};
}

static Bytes to_bytes(MonoArray* arr) {
  // TODO: assert element type is byte.
  return (Bytes){.ptr = mono_array_addr(arr, uint8_t, 0),
                 .len = mono_array_length(arr)};
}

static void free_string(String span) {
  free(span.ptr);
}

static MonoArray* stdb_buffer_consume(Buffer buf);

// __attribute__((import_module("spacetime"),
//                import_name("_create_table"))) extern uint16_t
// _create_table(const char* name,
//               size_t name_len,
//               const uint8_t* schema,
//               size_t schema_len,
//               uint32_t* out);

// static uint32_t stdb_create_table(MonoString* name_, MonoArray* schema_) {
//   String name = to_string(name_);
//   Bytes schema = to_bytes(schema_);

//   uint32_t out;
//   uint16_t result =
//       _create_table(name.ptr, name.len, schema.ptr, schema.len, &out);

//   free_string(name);

//   check_result(result);

//   return out;
// }

__attribute__((import_module("spacetime"),
               import_name("_get_table_id"))) extern uint16_t
_get_table_id(const char* name, size_t name_len, uint32_t* out);

static uint32_t stdb_get_table_id(MonoString* name_) {
  String name = to_string(name_);

  uint32_t out;
  uint16_t result = _get_table_id(name.ptr, name.len, &out);

  free_string(name);

  check_result(result);

  return out;
}

__attribute__((import_module("spacetime"),
               import_name("_create_index"))) extern uint16_t
_create_index(const char* index_name,
              size_t index_name_len,
              uint32_t table_id,
              uint8_t index_type,
              const uint8_t* col_ids,
              size_t col_len);

static void stdb_create_index(MonoString* index_name_,
                              uint32_t table_id,
                              uint8_t index_type,
                              MonoArray* col_ids_) {
  String index_name = to_string(index_name_);
  Bytes col_ids = to_bytes(col_ids_);

  uint16_t result = _create_index(index_name.ptr, index_name.len, table_id,
                                  index_type, col_ids.ptr, col_ids.len);

  free_string(index_name);

  check_result(result);
}

__attribute__((import_module("spacetime"),
               import_name("_iter_by_col_eq"))) extern uint16_t
_iter_by_col_eq(uint32_t table_id,
                uint32_t col_id,
                const uint8_t* value,
                size_t value_len,
                Buffer* out);

static MonoArray* stdb_iter_by_col_eq(uint32_t table_id,
                                      uint32_t col_id,
                                      MonoArray* value_) {
  Bytes value = to_bytes(value_);

  Buffer out;
  uint16_t result =
      _iter_by_col_eq(table_id, col_id, value.ptr, value.len, &out);

  check_result(result);

  return stdb_buffer_consume(out);
}

__attribute__((import_module("spacetime"),
               import_name("_insert"))) extern uint16_t
_insert(uint32_t table_id, uint8_t* row, size_t row_len);

static void stdb_insert(uint32_t table_id, MonoArray* row_) {
  Bytes row = to_bytes(row_);

  uint16_t result = _insert(table_id, row.ptr, row.len);

  check_result(result);
}

// __attribute__((import_module("spacetime"),
//                import_name("_delete_pk"))) extern uint16_t
// _delete_pk(uint32_t table_id, const uint8_t* pk, size_t pk_len);

// static void stdb_delete_pk(uint32_t table_id, MonoArray* pk_) {
//   Bytes pk = to_bytes(pk_);

//   uint16_t result = _delete_pk(table_id, pk.ptr, pk.len);

//   check_result(result);
// }

// __attribute__((import_module("spacetime"),
//                import_name("_delete_value"))) extern uint16_t
// _delete_value(uint32_t table_id, const uint8_t* row, size_t row_len);

// static void stdb_delete_value(uint32_t table_id, MonoArray* row_) {
//   Bytes row = to_bytes(row_);

//   uint16_t result = _delete_value(table_id, row.ptr, row.len);

//   check_result(result);
// }

__attribute__((import_module("spacetime"),
               import_name("_delete_by_col_eq"))) extern uint16_t
_delete_by_col_eq(uint32_t table_id,
                  uint32_t col_id,
                  const uint8_t* value,
                  size_t value_len,
                  uint32_t* out);

static uint32_t stdb_delete_by_col_eq(uint32_t table_id,
                                      uint32_t col_id,
                                      MonoArray* value_) {
  Bytes value = to_bytes(value_);

  uint32_t out;
  uint16_t result =
      _delete_by_col_eq(table_id, col_id, value.ptr, value.len, &out);

  check_result(result);

  return out;
}

// __attribute__((import_module("spacetime"),
//                import_name("_delete_range"))) extern uint16_t
// _delete_range(uint32_t table_id,
//               uint32_t col_id,
//               const uint8_t* range_start,
//               size_t range_start_len,
//               const uint8_t* range_end,
//               size_t range_end_len,
//               uint32_t* out);

// static uint32_t stdb_delete_range(uint32_t table_id,
//                                   uint32_t col_id,
//                                   MonoArray* range_start_,
//                                   MonoArray* range_end_) {
//   Bytes range_start = to_bytes(range_start_);
//   Bytes range_end = to_bytes(range_end_);

//   uint32_t out;
//   uint16_t result =
//       _delete_range(table_id, col_id, range_start.ptr, range_start.len,
//                     range_end.ptr, range_end.len, &out);

//   check_result(result);

//   return out;
// }

__attribute__((import_module("spacetime"),
               import_name("_iter_start"))) extern uint16_t
_iter_start(uint32_t table_id, BufferIter* out);

static void stdb_iter_start(uint32_t table_id, BufferIter* iter) {
  uint16_t result = _iter_start(table_id, iter);

  check_result(result);
}

__attribute__((import_module("spacetime"),
               import_name("_iter_start_filtered"))) extern uint16_t
_iter_start_filtered(uint32_t table_id,
                     const uint8_t* filter,
                     size_t filter_len,
                     BufferIter* out);

static void stdb_iter_start_filtered(uint32_t table_id,
                                     MonoArray* filter_,
                                     BufferIter* iter) {
  Bytes filter = to_bytes(filter_);

  uint16_t result =
      _iter_start_filtered(table_id, filter.ptr, filter.len, iter);

  check_result(result);
}

__attribute__((import_module("spacetime"),
               import_name("_iter_next"))) extern uint16_t
_iter_next(BufferIter iter, Buffer* out);

static MonoArray* stdb_iter_next(BufferIter iter) {
  Buffer out;
  uint16_t result = _iter_next(iter, &out);

  check_result(result);

  return stdb_buffer_consume(out);
}

__attribute__((import_module("spacetime"),
               import_name("_iter_drop"))) extern uint16_t
_iter_drop(BufferIter iter);

static void stdb_iter_drop(BufferIter* iter) {
  // Guard against attempts to double free
  // (e.g. once via Dispose and once via destructor).
  if (iter->handle == INVALID_HANDLE) {
    return;
  }

  uint16_t result = _iter_drop(*iter);

  iter->handle = INVALID_HANDLE;

  check_result(result);
}

__attribute__((import_module("spacetime"),
               import_name("_console_log"))) extern void
_console_log(uint8_t level,
             const char* target,
             size_t target_len,
             const char* filename,
             size_t filename_len,
             uint32_t line_number,
             const char* text,
             size_t text_len);

static void stdb_console_log(MonoString* text_,
                             uint8_t level,
                             MonoString* target_,
                             MonoString* filename_,
                             uint32_t line_number) {
  String text = to_string(text_);
  String target = to_string(target_);
  String filename = to_string(filename_);

  _console_log(level, target.ptr, target.len, filename.ptr, filename.len,
               line_number, text.ptr, text.len);

  free_string(text);
  free_string(target);
  free_string(filename);
}

__attribute__((import_module("spacetime"),
               import_name("_schedule_reducer"))) extern void
_schedule_reducer(const char* name,
                  size_t name_len,
                  const uint8_t* args,
                  size_t args_len,
                  uint64_t time,
                  ScheduleToken* out);

static void stdb_schedule_reducer(
    MonoString* name_,
    MonoArray* args_,
    // by-value uint64_t + other args corrupts stack in Mono's FFI for some
    // reason pass by pointer instead
    uint64_t* time_,
    ScheduleToken* out) {
  String name = to_string(name_);
  Bytes args = to_bytes(args_);
  uint64_t time = *time_;

  _schedule_reducer(name.ptr, name.len, args.ptr, args.len, time, out);

  free_string(name);
}

__attribute__((import_module("spacetime"),
               import_name("_cancel_reducer"))) extern void
_cancel_reducer(ScheduleToken token);

static void stdb_cancel_reducer(ScheduleToken* token) {
  _cancel_reducer(*token);
}

__attribute__((import_module("spacetime"),
               import_name("_buffer_len"))) extern size_t
_buffer_len(Buffer buf);

__attribute__((import_module("spacetime"),
               import_name("_buffer_consume"))) extern void
_buffer_consume(Buffer buf, uint8_t* into, size_t len);

static MonoArray* stdb_buffer_consume(Buffer buf) {
  if (buf.handle == INVALID_HANDLE) {
    return NULL;
  }
  size_t len = _buffer_len(buf);
  MonoArray* result =
      mono_array_new(mono_domain_get(), mono_get_byte_class(), len);
  _buffer_consume(buf, mono_array_addr(result, uint8_t, 0), len);
  return result;
}

__attribute__((import_module("spacetime"),
               import_name("_buffer_alloc"))) extern Buffer
_buffer_alloc(const uint8_t* data, size_t data_len);

#define ATTACH(name, target_name) \
  mono_add_internal_call("SpacetimeDB.Runtime::" target_name, name)

void mono_stdb_attach_bindings() {
  // ATTACH(stdb_create_table, "CreateTable");
  ATTACH(stdb_get_table_id, "GetTableId");
  ATTACH(stdb_create_index, "CreateIndex");
  ATTACH(stdb_iter_by_col_eq, "IterByColEq");
  ATTACH(stdb_insert, "Insert");
  // ATTACH(stdb_delete_pk, "DeletePk");
  // ATTACH(stdb_delete_value, "DeleteValue");
  ATTACH(stdb_delete_by_col_eq, "DeleteByColEq");
  // ATTACH(stdb_delete_range, "DeleteRange");
  ATTACH(stdb_iter_start, "BufferIterStart");
  ATTACH(stdb_iter_start_filtered, "BufferIterStartFiltered");
  ATTACH(stdb_iter_next, "BufferIterNext");
  ATTACH(stdb_iter_drop, "BufferIterDrop");
  ATTACH(stdb_console_log, "Log");
  ATTACH(stdb_schedule_reducer, "ScheduleReducer");
  ATTACH(stdb_cancel_reducer, "CancelReducer");
}

__attribute__((export_name("__describe_module__"))) Buffer
__describe_module__() {
  MonoArray* bytes_arr = (MonoArray*)INVOKE_DOTNET_METHOD(
      "SpacetimeDB.Runtime.dll", "SpacetimeDB.Module", "FFI", "DescribeModule",
      NULL);
  Bytes bytes = to_bytes(bytes_arr);
  return _buffer_alloc(bytes.ptr, bytes.len);
}

static Buffer return_result_buf(MonoObject* str) {
  if (str == NULL) {
    return (Buffer){.handle = INVALID_HANDLE};
  }
  char* cstr = mono_string_to_utf8((MonoString*)str);
  Buffer buf = _buffer_alloc((uint8_t*)cstr, strlen(cstr));
  free(cstr);
  return buf;
}

__attribute__((export_name("__call_reducer__"))) Buffer __call_reducer__(
    uint32_t id,
    Buffer sender_,
    uint64_t timestamp,
    Buffer args_) {
  MonoArray* sender = stdb_buffer_consume(sender_);
  MonoArray* args = stdb_buffer_consume(args_);

  return return_result_buf(INVOKE_DOTNET_METHOD(
      "SpacetimeDB.Runtime.dll", "SpacetimeDB.Module", "FFI", "CallReducer",
      NULL, &id, sender, &timestamp, args));
}

__attribute__((export_name("__identity_connected__"))) Buffer
__identity_connected__(Buffer sender_, uint64_t timestamp) {
  MonoArray* sender = stdb_buffer_consume(sender_);

  return return_result_buf(
      INVOKE_DOTNET_METHOD("SpacetimeDB.Runtime.dll", "SpacetimeDB", "Runtime",
                           "IdentityConnected", NULL, sender, &timestamp));
}

__attribute__((export_name("__identity_disconnected__"))) Buffer
__identity_disconnected__(Buffer sender_, uint64_t timestamp) {
  MonoArray* sender = stdb_buffer_consume(sender_);

  return return_result_buf(
      INVOKE_DOTNET_METHOD("SpacetimeDB.Runtime.dll", "SpacetimeDB", "Runtime",
                           "IdentityDisconnected", NULL, sender, &timestamp));
}

// Shims to avoid dependency on WASI in the generated Wasm file.

#include <stdlib.h>
#include <wasi/api.h>

// Based on
// https://github.com/WebAssembly/wasi-libc/blob/main/libc-bottom-half/sources/__wasilibc_real.c,

int32_t __imported_wasi_snapshot_preview1_args_get(int32_t arg0, int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_args_sizes_get(int32_t arg0,
                                                         int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_environ_get(int32_t arg0,
                                                      int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_environ_sizes_get(int32_t arg0,
                                                            int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_clock_res_get(int32_t arg0,
                                                        uint64_t* timestamp) {
  *timestamp = 1;
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_clock_time_get(int32_t arg0,
                                                         int64_t arg1,
                                                         int32_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_advise(int32_t arg0,
                                                    int64_t arg1,
                                                    int64_t arg2,
                                                    int32_t arg3) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_write(int32_t arg0,
                                                   int32_t arg1,
                                                   int32_t arg2,
                                                   int32_t arg3) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_allocate(int32_t arg0,
                                                      int64_t arg1,
                                                      int64_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_close(int32_t arg0) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_datasync(int32_t arg0) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_get(int32_t arg0,
                                                        int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_set_flags(int32_t arg0,
                                                              int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_set_rights(int32_t arg0,
                                                               int64_t arg1,
                                                               int64_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_filestat_get(int32_t arg0,
                                                          int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_filestat_set_size(int32_t arg0,
                                                               int64_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_filestat_set_times(int32_t arg0,
                                                                int64_t arg1,
                                                                int64_t arg2,
                                                                int32_t arg3) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_pread(int32_t arg0,
                                                   int32_t arg1,
                                                   int32_t arg2,
                                                   int64_t arg3,
                                                   int32_t arg4) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_prestat_get(int32_t arg0,
                                                         int32_t arg1) {
  // Return this value to indicate there are no further preopens to iterate
  // through
  return __WASI_ERRNO_BADF;
}

int32_t __imported_wasi_snapshot_preview1_fd_prestat_dir_name(int32_t arg0,
                                                              int32_t arg1,
                                                              int32_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_pwrite(int32_t arg0,
                                                    int32_t arg1,
                                                    int32_t arg2,
                                                    int64_t arg3,
                                                    int32_t arg4) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_read(int32_t arg0,
                                                  int32_t arg1,
                                                  int32_t arg2,
                                                  int32_t arg3) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_readdir(int32_t arg0,
                                                     int32_t arg1,
                                                     int32_t arg2,
                                                     int64_t arg3,
                                                     int32_t arg4) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_renumber(int32_t arg0,
                                                      int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_seek(int32_t arg0,
                                                  int64_t arg1,
                                                  int32_t arg2,
                                                  int32_t arg3) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_sync(int32_t arg0) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_tell(int32_t arg0, int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_create_directory(int32_t arg0,
                                                                int32_t arg1,
                                                                int32_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_filestat_get(int32_t arg0,
                                                            int32_t arg1,
                                                            int32_t arg2,
                                                            int32_t arg3,
                                                            int32_t arg4) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_filestat_set_times(
    int32_t arg0,
    int32_t arg1,
    int32_t arg2,
    int32_t arg3,
    int64_t arg4,
    int64_t arg5,
    int32_t arg6) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_link(int32_t arg0,
                                                    int32_t arg1,
                                                    int32_t arg2,
                                                    int32_t arg3,
                                                    int32_t arg4,
                                                    int32_t arg5,
                                                    int32_t arg6) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_open(int32_t arg0,
                                                    int32_t arg1,
                                                    int32_t arg2,
                                                    int32_t arg3,
                                                    int32_t arg4,
                                                    int64_t arg5,
                                                    int64_t arg6,
                                                    int32_t arg7,
                                                    int32_t arg8) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_readlink(int32_t arg0,
                                                        int32_t arg1,
                                                        int32_t arg2,
                                                        int32_t arg3,
                                                        int32_t arg4,
                                                        int32_t arg5) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_remove_directory(int32_t arg0,
                                                                int32_t arg1,
                                                                int32_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_rename(int32_t arg0,
                                                      int32_t arg1,
                                                      int32_t arg2,
                                                      int32_t arg3,
                                                      int32_t arg4,
                                                      int32_t arg5) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_symlink(int32_t arg0,
                                                       int32_t arg1,
                                                       int32_t arg2,
                                                       int32_t arg3,
                                                       int32_t arg4) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_path_unlink_file(int32_t arg0,
                                                           int32_t arg1,
                                                           int32_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_poll_oneoff(int32_t arg0,
                                                      int32_t arg1,
                                                      int32_t arg2,
                                                      int32_t arg3) {
  return 0;
}

_Noreturn void __imported_wasi_snapshot_preview1_proc_exit(int32_t arg0) {
  exit(arg0);
}

int32_t __imported_wasi_snapshot_preview1_sched_yield() {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_random_get(int32_t arg0,
                                                     int32_t arg1) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_sock_accept(int32_t arg0,
                                                      int32_t arg1,
                                                      int32_t arg2) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_sock_recv(int32_t arg0,
                                                    int32_t arg1,
                                                    int32_t arg2,
                                                    int32_t arg3,
                                                    int32_t arg4,
                                                    int32_t arg5) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_sock_send(int32_t arg0,
                                                    int32_t arg1,
                                                    int32_t arg2,
                                                    int32_t arg3,
                                                    int32_t arg4) {
  return 0;
}

int32_t __imported_wasi_snapshot_preview1_sock_shutdown(int32_t arg0,
                                                        int32_t arg1) {
  return 0;
}

#ifdef _REENTRANT
int32_t __imported_wasi_thread_spawn(int32_t arg0) {
  return 0;
}
#endif

void _start();

__attribute__((export_name("__preinit__10_init_csharp"))) void
__preinit__10_init_csharp() {
  _start();
}

// __attribute__((export_name("SPACETIME_ABI_VERSION"))) -
// doesn't work on non-functions, must specify on command line
const uint32_t SPACETIME_ABI_VERSION = /* 4.0 */ (4 << 16) | 0;
const uint8_t SPACETIME_ABI_VERSION_IS_ADDR = 1;
