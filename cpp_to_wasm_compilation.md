# C++ to WebAssembly (WASM) Compilation for Host C ABI Integration

This document outlines key aspects of compiling C++ code to WebAssembly (WASM) with a focus on integrating the WASM module with a host environment that exposes a C Application Binary Interface (ABI).

## 1. Toolchains

Two primary toolchain approaches exist for compiling C++ to WASM: Emscripten and direct Clang/LLVM.

### Emscripten

*   **Purpose:** Emscripten is a comprehensive compiler toolchain (using Clang/LLVM internally) designed to be a near drop-in replacement for standard C/C++ compilers like GCC or Clang. Its primary goal is to make it easier to port existing C/C++ codebases to run on the web, Node.js, or other WASM runtimes.
*   **Common Usage:** Developers typically use `emcc` (Emscripten's compiler frontend) to compile C/C++ code. Emscripten not only produces a `.wasm` file but also generates JavaScript "glue" code. This glue code handles many complexities, such as:
    *   Setting up the WASM module's memory.
    *   Providing implementations for common C standard library functions (e.g., `stdio`, `pthreads`, basic memory management like `malloc`/`free` via `_malloc`/`_free` exposed to JS).
    *   Emulating a POSIX-like environment, including a virtual filesystem (MEMFS, NODEFS).
    *   Facilitating JavaScript-C++ interoperation (e.g., calling C functions from JS and vice-versa).
*   **Bridging C++ to WASM:** Emscripten abstracts many low-level details of WASM. It provides convenient macros and functions (`EMSCRIPTEN_KEEPALIVE`, `ccall`, `cwrap`, `EM_ASM`, `EM_JS`) to manage exports and imports between WASM and the JavaScript host environment. It aims to make the WASM module behave much like a native executable or library in a familiar environment.

### Clang/LLVM Directly

*   **Purpose:** Clang, as part of the LLVM project, includes a WebAssembly backend. This allows C++ code to be compiled directly to WASM object files (`.o`) and then linked into a final `.wasm` module using `wasm-ld` (LLVM's WASM linker).
*   **Capabilities:**
    *   **Minimalism:** This approach produces highly optimized and minimal WASM modules, as it doesn't include Emscripten's extensive JS glue or POSIX emulation layers by default.
    *   **Control:** Developers have fine-grained control over what is imported and exported, memory layout (to some extent), and which standard library features are included (often requiring a WASM-specific libc like WASI libc or none at all).
    *   **WASI:** Often used for targeting the WebAssembly System Interface (WASI), which provides a standardized set of system calls for WASM modules to interact with the host in a non-browser context.
*   **Potential Complexities:**
    *   **No JS Glue:** Without Emscripten, there's no automatic JavaScript glue. All interactions with the host (beyond basic WASM imports/exports) must be managed manually.
    *   **Standard Library:** Most C++ standard library features that rely on OS services (filesystem, networking, threading, exceptions beyond WASM exceptions, etc.) won't work out-of-the-box. A WASM-compatible standard library (like `wasi-libc` for WASI, or parts of Emscripten's libc if used selectively) or custom implementations are needed.
    *   **Memory Management:** While WASM provides linear memory, functions like `malloc` and `free` are not inherently part of WASM. The module must either include its own memory allocator or import these functions from the host.
    *   **Syscalls:** Direct system calls are not available. Any interaction with the outside world must go through imported host functions.
    *   **Build Process:** Requires more manual setup of compiler and linker flags (e.g., `--target=wasm32`, `-nostdlib`, linker scripts for memory layout).

### Recommendation for SpacetimeDB SDK

For creating a C++ SDK for SpacetimeDB that needs to integrate with a host providing a C ABI, the **direct Clang/LLVM approach (possibly with WASI-SDK or a similar minimal SDK)** is generally recommended.

**Justification:**

1.  **C ABI Focus:** The requirement is to interface with a C ABI. Emscripten's primary target is often JavaScript/Web environments, and while it can produce WASM that interacts via C ABI, its extensive JS glue and runtime are often unnecessary and add overhead if the host is not a JS environment.
2.  **Minimalism & Control:** An SDK should ideally be lightweight and provide precise control over its interface. Direct Clang/LLVM allows for creating a minimal WASM module containing only the necessary SDK logic, without Emscripten's emulation layers. This results in smaller binary sizes and potentially faster load/instantiation times.
3.  **No Browser Dependencies:** SpacetimeDB's host is not necessarily a browser. Emscripten's default tooling is heavily geared towards browser environments (HTML generation, JS APIs). While it can be configured for other runtimes (like Node.js or standalone WASM), direct Clang/LLVM is more natural for non-web host environments.
4.  **Avoiding JS Glue Overhead:** If the host environment is not JavaScript-based, Emscripten's JS glue code is largely unused and adds unnecessary complexity and size. Direct Clang/LLVM avoids this.
5.  **Clearer Contract:** Using direct Clang/LLVM with explicit `import_module`/`import_name` and `export_name` attributes makes the contract between the WASM module and the C ABI host very clear and self-contained within the C++ code.

**Caveat:** If the C++ SDK code heavily relies on POSIX APIs or a substantial part of the C++ standard library that interacts with an OS (e.g., filesystem, networking, full-featured exceptions), using Emscripten with a minimal runtime configuration (e.g., `-sSTANDALONE_WASM` or targeting WASI) might reduce the porting effort. However, this often means the host then needs to provide the Emscripten-expected imports, which might deviate from a pure C ABI.

For a clean C ABI, direct Clang/LLVM with careful management of dependencies and a clear import/export strategy is generally preferable for SDK development.

## 2. Exporting Functions from C++ to WASM

To make C++ functions callable from the WASM host (which expects a C ABI), several mechanisms are used:

*   **`extern "C"`:** This linkage specification is crucial when compiling C++ code. It prevents C++ name mangling, ensuring that the function names in the WASM module are simple C-style names that the host can easily look up.

    ```cpp
    // In C++ code
    extern "C" {
        int add(int a, int b) {
            return a + b;
        }
    }
    ```

*   **Compiler Attributes for Export:**
    *   **`__attribute__((export_name("desired_export_name")))` (Clang/LLVM specific):** This attribute allows you to specify the exact name under which the function will be exported in the WASM module's export section. This is useful for controlling the external name independently of the C++ function name.
        ```cpp
        extern "C" __attribute__((export_name("my_sdk_add"))) int add_numbers(int a, int b) {
            return a + b;
        }
        // This function will be exported as "my_sdk_add" in the WASM module.
        ```
    *   **`EMSCRIPTEN_KEEPALIVE` (Emscripten specific):** When using Emscripten, this macro (defined in `<emscripten.h>`) signals to the Emscripten compiler that a function should not be dead-code-eliminated and should be made available for calling from JavaScript (and thus generally exported in the WASM module).
        ```cpp
        #include <emscripten.h> // Required for Emscripten

        extern "C" {
            EMSCRIPTEN_KEEPALIVE int subtract(int a, int b) {
                return a - b;
            }
        }
        // Emscripten will ensure this function is callable.
        ```
    *   **Linker Flags (e.g., `-Wl,--export-all` or `-sEXPORTED_FUNCTIONS`):**
        *   When using `wasm-ld` (often via `clang --target=wasm32`), you can use linker flags like `-Wl,--export-all` to export all externally visible functions. This is a broad approach.
        *   Emscripten uses `-sEXPORTED_FUNCTIONS=['_func1', '_func2']` to list functions that should be explicitly exported (note the leading underscore Emscripten often adds).

**Illustrative C++ Snippet (for direct Clang/LLVM):**

```cpp
// my_module.cpp

// Ensure C-style linkage to prevent name mangling
extern "C" {

// Export this function with the name "exported_add_integers"
__attribute__((export_name("exported_add_integers")))
int add_integers(int x, int y) {
    return x + y;
}

// Export this function with its C name "multiply_integers"
// (assuming a linker flag like --export-all or specific export via linker script)
// Or, more explicitly for clarity even with --export-all:
__attribute__((export_name("multiply_integers")))
int multiply_integers(int x, int y) {
    return x * y;
}

// This function will only be visible internally to the WASM module
// unless explicitly exported by other means (e.g. --export-all and it's not static).
// To ensure it's NOT exported if --export-all is used, mark it static.
static int internal_helper(int x) {
    return x * 2;
}

__attribute__((export_name("process_data")))
int process_data(int x) {
    return internal_helper(x) + 5;
}

} // extern "C"
```

To compile this with Clang for WASM (example):
```bash
clang --target=wasm32 -nostdlib -Wl,--no-entry -Wl,--export=exported_add_integers -Wl,--export=multiply_integers -Wl,--export=process_data -o my_module.wasm my_module.cpp
# Or, to export all non-static functions under their C name (unless overridden by export_name):
# clang --target=wasm32 -nostdlib -Wl,--no-entry -Wl,--export-all -o my_module.wasm my_module.cpp
```

## 3. Importing Host Functions into C++ (WASM)

A C++ WASM module can call functions provided by the host environment. The host exposes these functions, and the WASM module declares them as imports.

*   **`extern "C"`:** Just as with exports, `extern "C"` should be used for declaring imported functions to ensure C-style linkage and prevent name mangling issues if the C++ code tries to interpret them as C++ functions.

*   **Compiler Attributes for Import:**
    *   **`__attribute__((import_module("module_name")))`:** This attribute specifies the first part of the two-level namespace for WASM imports: the "module name". Often, this is "env" by convention for general-purpose host functions, but it can be any string defined by the host.
    *   **`__attribute__((import_name("function_name_in_host")))`:** This attribute specifies the second part of the namespace: the "field name" or the actual name by which the function is known in the host's export list for that module.

**Illustrative C++ Snippet (for direct Clang/LLVM):**

```cpp
// my_wasm_module.cpp

// Declare functions imported from the host environment
extern "C" {

// Import 'host_log_message' from module "env"
__attribute__((import_module("env")))
__attribute__((import_name("host_log_message")))
void host_log(const char* message, int length);

// Import 'host_get_time' from module "imports" (example of a different module name)
// with the specific import name "currentTimeUnix"
__attribute__((import_module("imports")))
__attribute__((import_name("currentTimeUnix")))
long long host_get_time();

} // extern "C"

// Function in WASM that uses the imported host functions
extern "C" __attribute__((export_name("do_work_and_log")))
void do_work_and_log() {
    const char* msg = "Hello from WASM!";
    host_log(msg, 16); // Actual length of "Hello from WASM!"

    long long currentTime = host_get_time();
    // Convert time to string and log it (simplified)
    char time_str[50];
    // sprintf might not be available unless you link a WASI libc or similar
    // For simplicity, let's assume a way to format it or log the number directly
    // For this example, we'll just send another message.
    const char* time_msg_prefix = "Current time: ";
    host_log(time_msg_prefix, 14);
    // In a real scenario, you'd format 'currentTime' into a string and log that.
}
```
When this module is instantiated, the host must provide implementations for `env.host_log_message` and `imports.currentTimeUnix`.

**Emscripten Context:**
When using Emscripten, imports are often handled by implementing the C function signature in a JavaScript library file (e.g., `my_js_lib.js`):
```javascript
// my_js_lib.js
mergeInto(LibraryManager.library, {
  host_log_message: function(messagePtr, length) {
    // Module.UTF8ToString(messagePtr, length) can be used if HEAP is accessible
    // Or, more generally, read from Module.HEAPU8
    let message = "";
    for (let i = 0; i < length; ++i) {
      message += String.fromCharCode(Module.HEAPU8[messagePtr + i]);
    }
    console.log("Host (JS): " + message);
  },
  currentTimeUnix: function() {
    return Date.now();
  }
});
```
And compiled with `emcc my_wasm_module.cpp --js-library my_js_lib.js ...`.
Alternatively, `EM_JS` or `EM_ASM` macros can be used directly in C++ code to define these interactions.

## 4. Memory Management Considerations

WebAssembly runs in a sandboxed environment with its own linear memory, separate from the host's memory. This separation is fundamental to WASM's security model. When data needs to be passed between the WASM module and the host, memory management becomes a critical consideration.

*   **WASM Linear Memory:** The WASM module has one (or more, with multi-memory proposal) block of linear memory. The module can read and write to this memory directly and efficiently. Its size can be fixed or growable.
*   **Host Memory:** The host environment (e.g., the runtime executing the WASM module) has its own memory space.
*   **Data Transfer:**
    *   **Simple Types:** Basic numeric types (integers, floats) can often be passed directly as function arguments or return values.
    *   **Complex Types (Buffers, Strings, Structs):** To pass complex data like a string, an array, or a C struct, a pointer to a region within the WASM module's linear memory is typically used.
        1.  **WASM to Host:** The WASM module can write data into its linear memory and pass a pointer (which is an integer offset into its memory) and the data's length to an imported host function. The host then needs to read the data from the WASM module's memory at that offset.
        2.  **Host to WASM:** The host can write data into the WASM module's linear memory (if the host has write access, e.g., via JavaScript `WebAssembly.Memory` object) and then call an exported WASM function with a pointer and length. Alternatively, the WASM module can export a function that allocates memory within its own space (e.g., `my_wasm_alloc(size)`), return the pointer to the host. The host then writes data into this allocated region and calls another WASM function to process it.

*   **Ownership and Lifetime:**
    *   **Who owns the memory?** If the WASM module allocates memory (e.g., using its internal `malloc` or a custom allocator), it owns that memory. If the host prepares a buffer for the WASM module to use, the ownership depends on the agreed ABI.
    *   **Allocation/Deallocation:**
        *   If a WASM module needs to work with dynamically sized data provided by the host, the host might provide allocation functions (like `_buffer_alloc` mentioned in the SpacetimeDB ABI). The WASM module would call `_buffer_alloc(size)` to request a buffer from the host. The host allocates this buffer (potentially in WASM linear memory if the host can write to it, or in host memory if data is copied later) and returns a pointer/handle.
        *   Conversely, if the WASM module generates data it needs to pass to the host, it might allocate space in its own linear memory and pass a pointer. The host then reads/consumes it.
        *   A corresponding deallocation function (e.g., `_buffer_consume` or `my_wasm_free`) is essential to prevent memory leaks. If the host allocated the buffer via `_buffer_alloc`, the WASM module might call `_buffer_consume(ptr)` to signal that it's done, and the host can then reclaim that memory. If the WASM module allocated it, it should typically expose a `my_wasm_free(ptr)` function for the host to call, or manage it internally.

*   **Why `_buffer_alloc` / `_buffer_consume` are Necessary:**
    Functions like `_buffer_alloc` and `_buffer_consume` (or similar patterns like `allocate_wasm_buffer` / `free_wasm_buffer`) are necessary because:
    1.  **Sandboxing:** The WASM module cannot directly access host memory or call arbitrary host system allocation functions unless explicitly imported.
    2.  **Decoupling:** They provide a clear contract for memory management across the WASM/host boundary, abstracting the actual allocation mechanism. The host decides where and how this "shared" buffer memory is actually allocated (it could be within the WASM linear memory if the host has write access, or a host-side buffer that requires copying).
    3.  **Control for the Host:** The host maintains control over memory resources, which is crucial for stability and security. The host can manage memory pools, enforce quotas, or perform other memory-related tasks.
    4.  **Lifetime Management:** Explicit allocation and consumption/deallocation functions clearly define the lifetime of shared buffers, preventing use-after-free or double-free errors at the boundary.

    Without such an explicit ABI for buffer management, passing anything more complex than simple numeric types would require either copying data back and forth (inefficient for large data) or relying on unsafe assumptions about memory layout and lifetime.

## 5. Data Types and Calling Conventions

When interfacing C++ WASM with a C ABI host:

*   **Directly Mappable Types:**
    *   **Integers:** `int8_t`, `uint8_t`, `int16_t`, `uint16_t`, `int32_t`, `uint32_t`, `int64_t`, `uint64_t` map directly to WASM types `i32` and `i64`. `char` (depending on its signedness) also maps. `size_t` usually maps to `i32` in wasm32.
    *   **Floating-point:** `float` and `double` map directly to WASM types `f32` and `f64`.
    *   **Pointers:** Pointers in C/C++ are represented as `i32` in wasm32 (or `i64` in wasm64). These are essentially byte offsets into the WASM module's linear memory. The host receives these as integers.

*   **Handling Complex C++ Types:**
    C++ types like `std::string`, `std::vector`, structs, and classes are not directly understood by the C ABI or WASM's external interface beyond their memory representation. They are typically handled as follows:
    *   **Pointers and Lengths:**
        *   `std::string`: Pass as `const char*` (obtained via `my_string.c_str()`) and a separate `int` or `size_t` for the length. The recipient (host or WASM) then reads that many bytes from the WASM linear memory. If the string needs to be modified or ownership transferred, the memory must be managed (copied or allocated via the defined ABI).
        *   `std::vector<T>`: Pass as a pointer to the first element (`my_vector.data()`) and the number of elements (`my_vector.size()`). The type `T` must also be ABI-compatible (e.g., primitive types or structs of primitive types).
        *   **Structs/Classes:**
            *   If a struct contains only ABI-compatible primitive types, its memory layout is generally consistent. It can be passed by pointer (as an `i32` offset). The host then reads the struct's members from WASM memory according to the expected layout.
            *   If a struct contains pointers (e.g., a `char*` in a struct), those pointers are offsets within the WASM linear memory. The host must dereference them accordingly.
            *   C++ classes with non-trivial constructors, destructors, virtual functions, or complex inheritance are generally not directly passable across the C ABI boundary. Usually, you'd expose C-style wrapper functions that operate on opaque pointers (handles) to class instances, or serialize/deserialize the class data.

    *   **Serialization/Deserialization:** For very complex data or when a stable ABI is paramount, data can be serialized into a defined format (e.g., JSON, Protocol Buffers, or a custom binary format) into a WASM memory buffer. A pointer to this buffer and its length are then passed. The other side deserializes the data. This adds overhead but provides flexibility and decouples the C++ type system from the host.

    *   **Memory Allocation for Host-Provided Data:** If the host needs to provide a string or array to WASM, the WASM module might export an `allocate_buffer(size)` function. The host calls this, gets a pointer (integer offset) into WASM memory, writes the data into that memory (e.g., using `WebAssembly.Memory.buffer` in JS), and then calls the WASM function with the pointer and size. The WASM module must also export a `free_buffer(ptr)` function.

**Example: Passing a struct and a string**

```cpp
// C++ (WASM Module)
struct MyData {
    int id;
    double value;
};

extern "C" {

__attribute__((import_module("env")))
__attribute__((import_name("process_host_data")))
void host_process_data(const MyData* data_ptr, const char* text_ptr, int text_len);

MyData global_data = { 10, 3.14 };
const char* global_text = "Sample Text";

__attribute__((export_name("send_data_to_host")))
void send_data_to_host() {
    host_process_data(&global_data, global_text, 11); // 11 is length of "Sample Text"
}

} // extern "C"
```

The host, when `host_process_data` is called, would receive:
*   `data_ptr`: An integer offset (e.g., 1024). It would read `sizeof(MyData)` bytes starting at this offset in the WASM linear memory and interpret it as `MyData { id_val, value_val }`.
*   `text_ptr`: An integer offset (e.g., 2048). It would read `text_len` bytes from this offset to get the string.
```
