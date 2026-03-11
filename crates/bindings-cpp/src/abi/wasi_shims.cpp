// WASI shims for SpacetimeDB C++ modules
// This file provides stub implementations of WASI functions to enable
// C++ standard library usage without requiring actual WASI support

#include <cstdint>
#include <cstddef>

// SpacetimeDB imports we need for console output
// Import from spacetime_10.0 module as required by SpacetimeDB ABI
extern "C" __attribute__((import_module("spacetime_10.0"), import_name("console_log")))
void console_log(uint8_t log_level, const uint8_t* target, uint32_t target_len,
                 const uint8_t* filename, uint32_t filename_len, uint32_t line_number,
                 const uint8_t* message, uint32_t message_len);

// Helper macro for string literals
#define CSTR(s) (uint8_t*)s, sizeof(s) - 1

// WASI types
struct __wasi_ciovec_t {
    const uint8_t* buf;
    size_t buf_len;
};

typedef uint32_t __wasi_fd_t;
typedef uint32_t __wasi_size_t;
typedef uint32_t __wasi_errno_t;

// File descriptors
#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

// WASI error codes
#define __WASI_ERRNO_SUCCESS 0
#define __WASI_ERRNO_BADF 8

// Macro to define WASI shim functions that always succeed
#define WASI_SHIM(name, params) \
    extern "C" __wasi_errno_t __wasi_##name params { return __WASI_ERRNO_SUCCESS; }

// Environment functions
WASI_SHIM(environ_get, (int32_t, int32_t))
WASI_SHIM(environ_sizes_get, (int32_t, int32_t))

// Clock functions
WASI_SHIM(clock_time_get, (int32_t, int64_t, int32_t))

// File descriptor functions
WASI_SHIM(fd_advise, (int32_t, int64_t, int64_t, int32_t))
WASI_SHIM(fd_allocate, (int32_t, int64_t, int64_t))
WASI_SHIM(fd_close, (int32_t))
WASI_SHIM(fd_datasync, (int32_t))
WASI_SHIM(fd_fdstat_get, (int32_t, int32_t))
WASI_SHIM(fd_fdstat_set_flags, (int32_t, int32_t))
WASI_SHIM(fd_fdstat_set_rights, (int32_t, int64_t, int64_t))
WASI_SHIM(fd_filestat_get, (int32_t, int32_t))
WASI_SHIM(fd_filestat_set_size, (int32_t, int64_t))
WASI_SHIM(fd_filestat_set_times, (int32_t, int64_t, int64_t, int32_t))
WASI_SHIM(fd_pread, (int32_t, int32_t, int32_t, int64_t, int32_t))
WASI_SHIM(fd_prestat_dir_name, (int32_t, int32_t, int32_t))
WASI_SHIM(fd_pwrite, (int32_t, int32_t, int32_t, int64_t, int32_t))
WASI_SHIM(fd_read, (int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(fd_readdir, (int32_t, int32_t, int32_t, int64_t, int32_t))
WASI_SHIM(fd_renumber, (int32_t, int32_t))
WASI_SHIM(fd_seek, (int32_t, int64_t, int32_t, int32_t))
WASI_SHIM(fd_sync, (int32_t))
WASI_SHIM(fd_tell, (int32_t, int32_t))

// Path functions
WASI_SHIM(path_create_directory, (int32_t, int32_t, int32_t))
WASI_SHIM(path_filestat_get, (int32_t, int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(path_filestat_set_times, (int32_t, int32_t, int32_t, int32_t, int64_t, int64_t, int32_t))
WASI_SHIM(path_link, (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(path_open, (int32_t, int32_t, int32_t, int32_t, int32_t, int64_t, int64_t, int32_t, int32_t))
WASI_SHIM(path_readlink, (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(path_remove_directory, (int32_t, int32_t, int32_t))
WASI_SHIM(path_rename, (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(path_symlink, (int32_t, int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(path_unlink_file, (int32_t, int32_t, int32_t))

// Other functions
WASI_SHIM(poll_oneoff, (int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(sched_yield, ())
WASI_SHIM(random_get, (int32_t, int32_t))
WASI_SHIM(sock_accept, (int32_t, int32_t, int32_t))
WASI_SHIM(sock_recv, (int32_t, int32_t, int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(sock_send, (int32_t, int32_t, int32_t, int32_t, int32_t))
WASI_SHIM(sock_shutdown, (int32_t, int32_t))

// Special handling for args_sizes_get and args_get
// We provide a dummy executable name to avoid issues
extern "C" {
const char executable_name[] = "stdb.wasm";

__wasi_errno_t __wasi_args_sizes_get(__wasi_size_t* argc, __wasi_size_t* argv_buf_size) {
    *argc = 1;
    *argv_buf_size = sizeof(executable_name);
    return __WASI_ERRNO_SUCCESS;
}

__wasi_errno_t __wasi_args_get(uint8_t** argv, uint8_t* argv_buf) {
    argv[0] = argv_buf;
    __builtin_memcpy(argv_buf, executable_name, sizeof(executable_name));
    return __WASI_ERRNO_SUCCESS;
}

// Clock resolution should be non-zero
__wasi_errno_t __wasi_clock_res_get(int32_t, uint64_t* timestamp) {
    *timestamp = 1;
    return __WASI_ERRNO_SUCCESS;
}

// Special handling for fd_write to avoid infinite loops
// Redirect output to console_log
__wasi_errno_t __wasi_fd_write(__wasi_fd_t fd, const __wasi_ciovec_t* iovs,
                        size_t iovs_len, __wasi_size_t* retptr0) {
    *retptr0 = 0;
    
    // Concatenate all iovs into a single buffer to avoid multiple log lines
    size_t total_len = 0;
    for (size_t i = 0; i < iovs_len; i++) {
        total_len += iovs[i].buf_len;
    }
    
    // Skip if nothing to write
    if (total_len == 0) {
        return __WASI_ERRNO_SUCCESS;
    }
    
    // Allocate a temporary buffer (on stack if small, heap if large)
    constexpr size_t STACK_BUFFER_SIZE = 1024;
    uint8_t stack_buffer[STACK_BUFFER_SIZE];
    uint8_t* buffer = stack_buffer;
    bool heap_allocated = false;
    
    if (total_len > STACK_BUFFER_SIZE) {
        buffer = new uint8_t[total_len];
        heap_allocated = true;
    }
    
    // Copy all iovs into the buffer
    size_t offset = 0;
    for (size_t i = 0; i < iovs_len; i++) {
        if (iovs[i].buf_len > 0) {
            __builtin_memcpy(buffer + offset, iovs[i].buf, iovs[i].buf_len);
            offset += iovs[i].buf_len;
            *retptr0 += iovs[i].buf_len;
        }
    }
    
    // Make a single console_log call with the complete message
    uint8_t log_level = (fd == STDERR_FILENO) ? 1 : 2; // 1=WARN, 2=INFO
    console_log(log_level, CSTR("wasi"), CSTR(__FILE__), __LINE__, 
               buffer, offset);
    
    // Clean up heap allocation if needed
    if (heap_allocated) {
        delete[] buffer;
    }
    
    return __WASI_ERRNO_SUCCESS;
}

// fd_prestat_get returns BADF to indicate end of iteration
__wasi_errno_t __wasi_fd_prestat_get(int32_t, int32_t) {
    return __WASI_ERRNO_BADF;
}

// Actually exit on proc_exit
[[noreturn]] void __wasi_proc_exit(int32_t code) {
    // In a WASM module, we can't actually exit
    // Mark the exit code as used (could be useful for debugging)
    (void)code;
    // Just spin forever
    while (true) {}
}

// Additional sock_accept for compatibility
__wasi_errno_t sock_accept(int32_t, int32_t, int32_t) {
    return __WASI_ERRNO_SUCCESS;
}

} // extern "C"

// Emscripten-specific shims
extern "C" {

// Emscripten runtime functions that might be imported
void emscripten_notify_memory_growth(int32_t) {
    // No-op - memory growth is handled by the runtime
}

} // extern "C"