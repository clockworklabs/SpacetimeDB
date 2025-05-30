# SpacetimeDB C++ SDK Documentation

## 1. Introduction

### Purpose
The SpacetimeDB C++ SDK empowers developers to build high-performance server-side application logic (modules) for SpacetimeDB using the C++ programming language. It provides a comprehensive suite of tools and libraries for defining data schemas (tables), implementing custom business logic (reducers), interacting with the underlying database, handling efficient data serialization, and integrating with a familiar CMake-based build system. The final output is a WebAssembly (WASM) module, designed to run securely and efficiently within the SpacetimeDB server environment.

This SDK is tailored for C++ developers who want to leverage the performance and control of C++ while building scalable and real-time applications on SpacetimeDB.

### Overview of SDK Features
*   **Schema Definition:** Define your database tables using C++ structs or classes. Register them with the SDK to make them accessible for database operations.
*   **Reducer Implementation:** Write your core application logic as C++ functions (reducers). These reducers can be invoked by clients or other internal game logic to effect state changes.
*   **Data Serialization (BSATN):** Automatic and manual data serialization to and from BSATN (Binary SpacetimeDB Abstract Type Notation), the native binary format for SpacetimeDB, ensuring efficient data transfer and storage.
*   **Database Interaction API:** A user-friendly C++ API (`Database` and `Table<T>` classes) for common database operations such as inserting rows, deleting rows by primary key or other criteria, and querying data using iterators or specific filters.
*   **Build System Integration:** Utilizes CMake for building C++ modules into WebAssembly. The SDK provides conventions and a toolchain file for seamless integration with Emscripten (the C++ to WASM compiler).
*   **CLI Compatibility:** Projects are structured to be compatible with the `spacetime` CLI for easy publishing, mimicking the conventions used by Rust-based SpacetimeDB modules.
*   **Contextual Information:** Reducers receive a `ReducerContext` object providing access to crucial transaction information, such as the sender's identity and the transaction timestamp.
*   **Low-level ABI Access:** For advanced use cases, the underlying C ABI functions provided by the SpacetimeDB host are accessible.

### Prerequisites
Before you begin developing SpacetimeDB modules with the C++ SDK, ensure you have the following tools installed and configured:

*   **C++17 Compiler:** A modern C++ compiler that supports at least the C++17 standard (e.g., Clang, GCC, MSVC for local development, though Emscripten will be the final compiler for WASM).
*   **Emscripten (emsdk):** This is the compiler toolchain used to compile C++ to WebAssembly.
    *   Download and install the EMSDK from the [official Emscripten documentation](https://emscripten.org/docs/getting_started/downloads.html).
    *   Ensure that the Emscripten environment is active in your terminal (e.g., by sourcing `emsdk_env.sh` or `emsdk_env.bat`) so that `emcc` and `em++` are in your system's PATH, or that the `EMSDK` environment variable is set to the root of your emsdk installation.
*   **CMake:** Version 3.15 or higher. CMake is used to manage the build process for both the SDK and your C++ modules.
*   **SpacetimeDB CLI:** The `spacetime` command-line tool for managing, running, and publishing your SpacetimeDB databases and modules.
*   **Ninja (Recommended):** While not strictly required, Ninja is a fast build system that works well with CMake and is often recommended for C++ projects, including those using Emscripten.

## 2. Getting Started

This section will guide you through setting up a new C++ SpacetimeDB module project, building it into a WASM file, and publishing it.

### Project Setup

We recommend the following general project structure:

```
spacetime_cpp_project_root/
├── sdk/                      # SpacetimeDB C++ SDK source and build files
│   ├── include/              # SDK public headers (e.g., spacetimedb/sdk/database.h)
│   ├── src/                  # SDK source files (e.g., database.cpp)
│   └── CMakeLists.txt        # CMake file for building the SDK static library
├── examples/
│   └── quickstart_cpp_kv/    # Example C++ module (your project would be similar)
│       ├── CMakeLists.txt    # CMake file for building this specific module
│       ├── Cargo.toml        # Dummy Cargo.toml for `spacetime` CLI compatibility
│       └── src/
│           ├── kv_store.h
│           └── kv_store.cpp
├── toolchains/
│   └── wasm_toolchain.cmake  # CMake toolchain file for Emscripten/WASM
└── build_and_publish_example.sh # Example script to build and publish the quickstart
```

#### 2.1. `Cargo.toml` for Your Module
In the root directory of your C++ module (e.g., `examples/quickstart_cpp_kv/`), create a `Cargo.toml` file. This file is **not** used to build your C++ code with Rust's Cargo build system. Instead, it serves as a metadata file that the `spacetime` CLI uses to identify your project, its name, and the conventional location of the compiled WASM artifact.

**Example: `examples/quickstart_cpp_kv/Cargo.toml`**
```toml
[package]
name = "kvstore_module" # Crucial: This name will be used by the CLI and build scripts.
version = "0.1.0"
edition = "2021" # Or any relevant Rust edition, e.g., "2018"

# This Cargo.toml file is primarily for compatibility with the `spacetime publish` CLI.
# It helps the CLI identify the project name and expected output path for the pre-compiled WASM module.
# The C++ code itself is built using CMake and a C++ toolchain (e.g., Emscripten).

[lib]
crate-type = ["cdylib"] # Indicates a dynamic system library, commonly used for WASM modules.
```
Ensure the `name` field matches the intended output name for your WASM module.

#### 2.2. `CMakeLists.txt` for Your Module
In your module's root directory (e.g., `examples/quickstart_cpp_kv/`), create a `CMakeLists.txt` file. This file will define how your C++ module is built.

**Key aspects:**
*   Set the C++ standard to 17 or higher.
*   Define a variable `MODULE_NAME` that exactly matches the `name` in your `Cargo.toml`.
*   Specify your C++ source files.
*   Configure the output directory for the WASM file to be `target/wasm32-unknown-unknown/release/` relative to this `CMakeLists.txt` (and `Cargo.toml`).
*   Ensure the final WASM filename is `${MODULE_NAME}.wasm`.
*   Link against the SpacetimeDB C++ SDK library. The example below shows how to do this using `add_subdirectory` if your module is part of a larger project that includes the SDK. If you've installed the SDK elsewhere, you might use `find_package(SpacetimeDBCppSDK)`.

**Example: `examples/quickstart_cpp_kv/CMakeLists.txt`**
```cmake
cmake_minimum_required(VERSION 3.15)
project(KvStoreModuleUserProject CXX) # This CMake project name can be different

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Define the module name, this MUST match the 'name' in Cargo.toml
set(MODULE_NAME "kvstore_module") # Matches Cargo.toml name

# Set the output directory and filename to match Rust's convention
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/target/wasm32-unknown-unknown/release)

# Add module source files
add_executable(${MODULE_NAME}
    src/kv_store.cpp
)

# Set the output name and suffix for the WASM file
set_target_properties(${MODULE_NAME} PROPERTIES OUTPUT_NAME "${MODULE_NAME}")
set_target_properties(${MODULE_NAME} PROPERTIES SUFFIX ".wasm")

# --- SpacetimeDB C++ SDK Linking ---
# Assuming this example module is in 'spacetime_cpp_project_root/examples/quickstart_cpp_kv/'
# and the SDK is in 'spacetime_cpp_project_root/sdk/'
get_filename_component(PROJECT_ROOT_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../ ABSOLUTE)
set(SPACETIMEDB_SDK_DIR ${PROJECT_ROOT_DIR}/sdk CACHE PATH "Path to SpacetimeDB C++ SDK root directory")

if(NOT IS_DIRECTORY ${SPACETIMEDB_SDK_DIR})
    message(FATAL_ERROR "SpacetimeDB SDK directory not found: ${SPACETIMEDB_SDK_DIR}.")
endif()

# Include the SDK's CMakeLists.txt to make its targets available
# The second argument is a binary directory for the SDK's build artifacts within this project's build tree.
# EXCLUDE_FROM_ALL prevents the SDK from being built unless this module (or another target) depends on it.
add_subdirectory(${SPACETIMEDB_SDK_DIR} ${CMAKE_BINARY_DIR}/sdk_build EXCLUDE_FROM_ALL)

# Link against the SDK's public target (e.g., spacetimedb_cpp_sdk or spacetimedb::sdk::spacetimedb_cpp_sdk)
# The SDK's CMakeLists.txt should define 'spacetimedb_cpp_sdk' as an alias or interface library.
target_link_libraries(${MODULE_NAME} PUBLIC spacetimedb_cpp_sdk)

# Include directories for this module (e.g., its own 'src' directory for local headers)
target_include_directories(${MODULE_NAME} PUBLIC src)
# The SDK's include directories should be automatically propagated by target_link_libraries
# if the SDK target correctly sets its INTERFACE_INCLUDE_DIRECTORIES.
# --- End SDK Linking ---

message(STATUS "Building user module: ${MODULE_NAME}.wasm")
message(STATUS "Output directory: ${CMAKE_RUNTIME_OUTPUT_DIRECTORY}")
message(STATUS "To configure: cmake -B build -S . -DCMAKE_TOOLCHAIN_FILE=<path_to_toolchain>")
message(STATUS "To build: cmake --build build")
```

#### 2.3. `wasm_toolchain.cmake`
This CMake toolchain file configures CMake to use Emscripten for compiling to WebAssembly. It should be located in a known path, for example, `toolchains/wasm_toolchain.cmake` at the root of your overall project.

*(Refer to the `toolchains/wasm_toolchain.cmake` content from Step 4 of the Materialize plan (originally Step 6 of initial plan) for its full content. It handles finding `emcc`/`em++`, setting `CMAKE_SYSTEM_NAME` to `Emscripten`, and configuring appropriate compile/link flags like `-s SIDE_MODULE=1` and `--no-entry`.)*

### Building the Module

You can build your module manually using CMake commands or use the provided example script.

#### Manual CMake Build (from your module's directory, e.g., `examples/quickstart_cpp_kv/`)
1.  **Configure CMake:**
    ```bash
    cmake -B build -S . -DCMAKE_TOOLCHAIN_FILE=../../toolchains/wasm_toolchain.cmake
    ```
    (Adjust the path to `wasm_toolchain.cmake` based on your project structure.)

2.  **Compile:**
    ```bash
    cmake --build build
    ```
    This will compile your C++ code and link it with the SpacetimeDB C++ SDK, producing the WASM file in `target/wasm32-unknown-unknown/release/`.

#### Using `build_and_publish_example.sh`
An example script, `build_and_publish_example.sh`, is provided at the root of the SDK project. This script automates the build and publish process for the `quickstart_cpp_kv` example.

*(Refer to the `build_and_publish_example.sh` script from Step 5 of the Materialize plan (originally Step 7 of initial plan). It `cd`s into the example directory, runs CMake configuration and build, checks for the output, and then publishes.)*

### Publishing the Module
After successfully building your `.wasm` file into the `target/wasm32-unknown-unknown/release/` directory within your module's project folder:

1.  Ensure you are in your module's root directory (e.g., `examples/quickstart_cpp_kv/`), where your `Cargo.toml` is located.
2.  Run the `spacetime publish` command:

    ```bash
    spacetime publish <module_name_from_cargo_toml> --name <your_db_address_or_alias>
    ```
    For example, if your `Cargo.toml` has `name = "kvstore_module"`:
    ```bash
    spacetime publish kvstore_module --name my_kv_database_on_cloud
    ```
The `spacetime` CLI uses the `<module_name_from_cargo_toml>` to find the `Cargo.toml` file, reads the module name, and expects the compiled WASM artifact at the conventional path (`target/wasm32-unknown-unknown/release/<module_name>.wasm`). It then uploads this WASM file to SpacetimeDB.

## 3. Core SDK Concepts

The SpacetimeDB C++ SDK is designed around a few core concepts: defining your data schema with C++ types, writing business logic in C++ functions called reducers, and interacting with the database through an SDK-provided API. All SDK components are typically found under the `spacetimedb::sdk` namespace, with BSATN utilities under `spacetimedb::bsatn`, and low-level ABI functions under `spacetimedb::abi` (though direct ABI use is rare). SDK headers are typically included like `#include <spacetimedb/sdk/database.h>`.

### Defining Tables

Tables store your application's persistent data. You define the structure of each table row using C++ structs or classes.

#### 3.1. C++ Structs/Classes for Tables
```cpp
// src/kv_store.h (example from quickstart_cpp_kv)
#include <spacetimedb/sdk/spacetimedb_sdk_types.h>
#include <spacetimedb/bsatn/bsatn.h>      // For BsatnSerializable, bsatn_writer, bsatn_reader
#include <string>
#include <cstdint>

namespace spacetimedb_quickstart {

struct KeyValue : public spacetimedb::sdk::bsatn::BsatnSerializable {
    std::string key_str;   // Primary Key
    std::string value_str;

    KeyValue(std::string k = "", std::string v = "") : key_str(std::move(k)), value_str(std::move(v)) {}

    void bsatn_serialize(spacetimedb::sdk::bsatn::bsatn_writer& writer) const override {
        writer.write_string(key_str);   // Field order matters for PK assumption
        writer.write_string(value_str);
    }

    void bsatn_deserialize(spacetimedb::sdk::bsatn::bsatn_reader& reader) override {
        key_str = reader.read_string();
        value_str = reader.read_string();
    }
};

} // namespace spacetimedb_quickstart
```
**Key Requirements:**
*   Your struct/class must either:
    *   Inherit from `spacetimedb::sdk::bsatn::BsatnSerializable` and override the pure virtual `bsatn_serialize` and `bsatn_deserialize` methods.
    *   Or, provide public member functions with the exact signatures:
        *   `void bsatn_serialize(spacetimedb::sdk::bsatn::bsatn_writer& writer) const;`
        *   `void bsatn_deserialize(spacetimedb::sdk::bsatn::bsatn_reader& reader);`
*   These methods define how your C++ type is converted to and from the BSATN binary format. The order of `write_*` calls in `bsatn_serialize` must exactly match the order of `read_*` calls in `bsatn_deserialize`.

#### 3.2. Registering Tables
To make your C++ type usable as a SpacetimeDB table, you must register it using the `SPACETIMEDB_REGISTER_TABLE` macro. This macro is defined in `<spacetimedb/sdk/spacetimedb_sdk_table_registry.h>`.

Place this macro call in one of your `.cpp` files at the global scope (or within your module's namespace).

```cpp
// src/kv_store.cpp
#include "kv_store.h" // Where KeyValue is defined
#include <spacetimedb/sdk/spacetimedb_sdk_table_registry.h>

// Register the KeyValue table
SPACETIMEDB_REGISTER_TABLE(spacetimedb_quickstart::KeyValue, "kv_pairs", "key_str");
```
**Macro Parameters:**
*   **`CppStructType`**: The fully qualified C++ type for the table row (e.g., `spacetimedb_quickstart::KeyValue`).
*   **`"db_table_name"`**: A string literal representing the name this table will have within the SpacetimeDB database (e.g., `"kv_pairs"`).
*   **`"pk_field_name"`**: A string literal naming the field in your C++ struct that serves as the primary key (e.g., `"key_str"`).
    *   **Primary Key Convention:** The SDK currently assumes that the field named here as the primary key is the **first field serialized** in your `bsatn_serialize` method. This means its column index for SDK operations (like `delete_by_col_eq` when targeting the PK) will be `0`. If no primary key field name is provided (i.e., an empty string `""`), the table is registered without a designated primary key in the SDK's metadata.

This registration allows the SDK to map C++ types to database table names and understand their basic structure, particularly the primary key.

### Writing Reducers

Reducers are the heart of your module's logic. They are C++ functions that execute atomically and can modify database state.

#### 3.3.1. Reducer Function Signature
A reducer is a C++ function that takes a `spacetimedb::sdk::ReducerContext&` as its first argument, followed by any number of arguments that your application logic requires.

```cpp
// src/kv_store.cpp (example from quickstart_cpp_kv)
#include <spacetimedb/sdk/reducer_context.h>
#include <spacetimedb/abi/spacetimedb_abi.h> // For _console_log
#include <string>
#include <cstdint> // For uint8_t

// (Assuming LOG_LEVEL_INFO is defined, e.g., in kv_store.h)
// const uint8_t LOG_LEVEL_INFO = 3;

namespace spacetimedb_quickstart {
void kv_put(spacetimedb::sdk::ReducerContext& ctx, const std::string& key, const std::string& value) {
    // ... implementation ...
    std::string message = "kv_put called with key: " + key;
    _console_log(LOG_LEVEL_INFO, nullptr, 0, nullptr, 0, 0,
                 reinterpret_cast<const uint8_t*>(message.c_str()), message.length());
}
} // namespace spacetimedb_quickstart
```
*   The first argument **must** be `spacetimedb::sdk::ReducerContext& ctx`.
*   All subsequent arguments must be types supported by BSATN (primitives, `std::string`, `std::vector<uint8_t>`, `spacetimedb::sdk::Identity`, `spacetimedb::sdk::Timestamp`, or custom types that are `BsatnSerializable` or have the duck-typed BSATN methods).

#### 3.3.2. Registering Reducers
To make a C++ function callable as a SpacetimeDB reducer, you must register it using the `SPACETIMEDB_REDUCER` or `SPACETIMEDB_REDUCER_NO_ARGS` macros. These are defined in `<spacetimedb/sdk/spacetimedb_sdk_reducer.h>`.

Place these macro calls in a `.cpp` file at global scope (or within your module's namespace, ensuring the function name is fully qualified).

```cpp
// src/kv_store.cpp
#include "kv_store.h" // For reducer function declarations if not in this file
#include <spacetimedb/sdk/spacetimedb_sdk_reducer.h>

// ... (definition of kv_put, kv_get, kv_del) ...

// Register reducers
SPACETIMEDB_REDUCER(spacetimedb_quickstart::kv_put, const std::string&, const std::string&);
SPACETIMEDB_REDUCER(spacetimedb_quickstart::kv_get, const std::string&);

namespace spacetimedb_quickstart {
void my_simple_reducer(spacetimedb::sdk::ReducerContext& ctx) { /* ... */ }
}
SPACETIMEDB_REDUCER_NO_ARGS(spacetimedb_quickstart::my_simple_reducer);
```
*   **`SPACETIMEDB_REDUCER(REDUCER_FUNC_NAME, ...ARGS)`**:
    *   `REDUCER_FUNC_NAME`: The fully qualified C++ name of your reducer function.
    *   `...ARGS`: A comma-separated list of the C++ types of the arguments your reducer function takes (excluding the initial `ReducerContext&`).
*   **`SPACETIMEDB_REDUCER_NO_ARGS(REDUCER_FUNC_NAME)`**: Used for reducers that only take `ReducerContext&`.

**Macro Functionality:**
*   **WASM Export:** The macro generates an `extern "C"` wrapper function that is exported from the WASM module with a name matching your C++ reducer function name (e.g., `kv_put`).
*   **Argument Handling:**
    *   The SpacetimeDB host calls this exported wrapper with a single BSATN-encoded buffer.
    *   This buffer is expected to contain the sender's `Identity` and the transaction `Timestamp` first, followed by the actual user-defined arguments for your reducer, all serialized in order.
    *   The wrapper deserializes the `Identity` and `Timestamp` to create the `ReducerContext`.
    *   It then deserializes each subsequent user argument from the buffer using the types you specified in the macro.
*   **Error Handling:** If your C++ reducer function throws a `std::exception`, the wrapper catches it, logs an error message to the host (using `_console_log`), and returns a non-zero `uint16_t` error code to indicate failure. Uncaught non-`std::exception` types are also caught with a generic error message. If the reducer completes without an exception, `0` (success) is returned.

### `ReducerContext` Usage
The `ReducerContext` (`ctx`) is your primary interface to transaction-specific information and database operations within a reducer. It's defined in `<spacetimedb/sdk/reducer_context.h>`.

*   **`const spacetimedb::sdk::Identity& ctx.get_sender() const;`**
    Returns the `Identity` of the client or principal that initiated the current transaction.
*   **`spacetimedb::sdk::Timestamp ctx.get_timestamp() const;`**
    Returns the `Timestamp` (a `uint64_t` milliseconds since epoch) at which the current transaction is executing.
*   **`spacetimedb::sdk::Database& ctx.db();`**
    Returns a reference to the `Database` object, allowing you to access table operations.

### Database Operations
The SDK provides `Database` and `Table<T>` classes for interacting with your data. These are defined in `<spacetimedb/sdk/database.h>` and `<spacetimedb/sdk/table.h>`.

#### 3.5.1. Getting a Table Instance
First, get a `Database` reference from the `ReducerContext`, then get a `Table<T>` instance:
```cpp
// Assuming MyPlayer is a registered C++ type for table "players"
auto player_table = ctx.db().get_table<my_module_namespace::MyPlayer>("players");
```
This call uses the `_get_table_id` ABI function to resolve `"players"` to an internal table ID.

#### 3.5.2. Inserting Rows
```cpp
my_module_namespace::MyPlayer new_player;
new_player.player_id = 123; // Assuming player_id is the PK
new_player.username = "PlayerOne";
new_player.score = 0;

player_table.insert(new_player);
// If 'player_id' was auto-generated by the database and the ABI supports it,
// 'new_player.player_id' would be updated here after the call.
// Our current ABI for _insert allows the host to modify the provided buffer.
```
The `insert` method serializes `new_player` to BSATN and calls the `_insert` ABI function. The `new_player` object is passed by non-const reference because the host might modify the underlying buffer (e.g., to fill in an auto-generated primary key), and the SDK will deserialize these changes back into your `new_player` object.

#### 3.5.3. Deleting Rows by Column Value
```cpp
// Delete player where player_id (column 0, our PK) is 123
uint32_t pk_column_idx = 0; // By convention from SPACETIMEDB_REGISTER_TABLE
uint64_t player_id_to_delete = 123;
uint32_t num_deleted = player_table.delete_by_col_eq(pk_column_idx, player_id_to_delete);

if (num_deleted > 0) {
    // Log success
}
```
This uses the `_delete_by_col_eq` ABI function. `ValueType` must be BSATN-serializable.

#### 3.5.4. Iterating Over a Table (Full Scan)
```cpp
for (const my_module_namespace::MyPlayer& player : player_table.iter()) {
    // Access player.player_id, player.username, player.score
    std::string msg = "Iterating player: " + player.username;
     _console_log(LOG_LEVEL_DEBUG, nullptr, 0, nullptr, 0, 0, reinterpret_cast<const uint8_t*>(msg.c_str()), msg.length());
}
```
`player_table.iter()` returns a `spacetimedb::sdk::TableIterator<MyPlayer>`. The iterator handles calling `_iter_next`, `_buffer_consume`, and deserializing rows. The iterator automatically calls `_iter_drop` when it goes out of scope.

#### 3.5.5. Finding Rows by Column Value
```cpp
uint64_t target_id = 456;
uint32_t pk_idx = 0; // Assuming player_id is PK at index 0
std::vector<my_module_namespace::MyPlayer> found_players = player_table.find_by_col_eq(pk_idx, target_id);

for (const auto& player : found_players) {
    // Process player
}
```
`find_by_col_eq` uses the `_iter_by_col_eq` ABI function, which returns a buffer of concatenated BSATN-encoded rows. The SDK deserializes these into a `std::vector<T>`.

### Logging
For logging within your C++ module, you can directly use the `_console_log` ABI function, which is declared in `<spacetimedb/abi/spacetimedb_abi.h>`.

```cpp
#include <spacetimedb/abi/spacetimedb_abi.h> // For _console_log
#include <string>

// Define log levels (or use those from your header, e.g. kv_store.h)
// const uint8_t LOG_LEVEL_INFO_EXAMPLE = 3; // (As defined in kv_store.h example)

void my_function_with_logging() {
    std::string message = "This is an informational log message.";
    _console_log(3, // LOG_LEVEL_INFO
                 nullptr, 0,                // target_ptr, target_len (optional module path)
                 nullptr, 0,                // filename_ptr, filename_len (optional)
                 0,                         // line_number (optional)
                 reinterpret_cast<const uint8_t*>(message.c_str()),
                 message.length());
}
```
The `target`, `filename`, and `line_number` parameters are optional and can be `nullptr` and `0` if not used.

### Supported Data Types for Reducer Arguments and Table Fields
The C++ SDK directly supports serialization/deserialization for:
*   **Primitives:** `bool`, `uint8_t`, `uint16_t`, `uint32_t`, `uint64_t`, `int8_t`, `int16_t`, `int32_t`, `int64_t`, `float` (`f32`), `double` (`f64`).
*   **Strings:** `std::string` (serialized as UTF-8 bytes with a `uint32_t` length prefix).
*   **Byte Arrays:** `std::vector<uint8_t>` (serialized with a `uint32_t` length prefix).
*   **Collections:** `std::vector<T>`, where `T` is any other supported BSATN-serializable type (including primitives, strings, `std::vector<uint8_t>`, or custom structs/classes).
*   **SDK-Specific Types:**
    *   `spacetimedb::sdk::Identity` (from `<spacetimedb/sdk/spacetimedb_sdk_types.h>`)
    *   `spacetimedb::sdk::Timestamp` (from `<spacetimedb/sdk/spacetimedb_sdk_types.h>`)
*   **Custom Serializable Types:** Any C++ struct or class that implements the `spacetimedb::bsatn::BsatnSerializable` interface or provides the necessary `bsatn_serialize` and `bsatn_deserialize` methods.

## 4. KeyValueStore Example Walkthrough

The `examples/quickstart_cpp_kv/` directory in the SDK project provides a simple key-value store module. This example demonstrates many of the core SDK concepts.

*   **`Cargo.toml`:** Defines `name = "kvstore_module"`.
*   **`CMakeLists.txt`:** Configures the build for `kvstore_module.wasm`, linking the SDK and outputting to `target/wasm32-unknown-unknown/release/`.
*   **`src/kv_store.h`:**
    *   Defines the `spacetimedb_quickstart::KeyValue` struct (with `key_str`, `value_str`) inheriting from `BsatnSerializable`.
    *   Declares reducer functions: `kv_put`, `kv_get`, `kv_del`.
*   **`src/kv_store.cpp`:**
    *   Implements `KeyValue::bsatn_serialize` and `KeyValue::bsatn_deserialize`.
    *   Registers the table: `SPACETIMEDB_REGISTER_TABLE(spacetimedb_quickstart::KeyValue, "kv_pairs", "key_str");`
    *   Implements `kv_put` (simulates upsert with delete then insert), `kv_get` (uses `find_by_col_eq`), and `kv_del` (uses `delete_by_col_eq`). Logging is done via a helper calling `_console_log`.
    *   Registers reducers using `SPACETIMEDB_REDUCER`.

**Build and Publish the Example:**
(Assuming you are in the `spacetime_cpp_project_root` directory where `build_and_publish_example.sh` is located)
```bash
./build_and_publish_example.sh my_kv_db_instance_name
```
This script will:
1.  `cd` into `examples/quickstart_cpp_kv`.
2.  Configure and build the WASM module using CMake and the Emscripten toolchain file. The output will be `examples/quickstart_cpp_kv/target/wasm32-unknown-unknown/release/kvstore_module.wasm`.
3.  Publish the module to SpacetimeDB using `spacetime publish kvstore_module --name my_kv_db_instance_name`.

**Example `spacetime call` commands:**
```bash
spacetime call my_kv_db_instance_name kv_put --key "player:1" --value "{\"name\":\"Alice\",\"score\":100}"
spacetime call my_kv_db_instance_name kv_get --key "player:1"
spacetime call my_kv_db_instance_name kv_del --key "player:1"
spacetime call my_kv_db_instance_name kv_get --key "player:1"
```
*(Note: The `--key <value>` syntax for `spacetime call` arguments assumes the CLI can map these named arguments to the BSATN-encoded buffer expected by the reducer. The C++ SDK's reducer macro expects arguments in the order they are defined in the C++ function signature, after the initial `Identity` and `Timestamp`.)*

## 5. Advanced Topics

*   **C ABI (`<spacetimedb/abi/spacetimedb_abi.h>`):** The C++ SDK is built upon a low-level C Application Binary Interface (ABI). This ABI defines a set of `extern "C"` functions (like `_console_log`, `_insert`, `_get_table_id`, etc.) that the WASM module imports from the SpacetimeDB host environment. While you typically interact with the higher-level C++ classes, understanding that this C ABI exists can be helpful for debugging or advanced scenarios.
*   **BSATN Serialization (`<spacetimedb/bsatn/bsatn.h>`):** All data exchanged with the host (reducer arguments, table rows) and stored in the database is serialized using BSATN. The SDK provides `bsatn_writer` and `bsatn_reader` classes to handle this. Custom C++ types need to be made BSATN-serializable.
*   **Reducer Macro Expansion (Conceptual):** The `SPACETIMEDB_REDUCER` macros are powerful tools that generate significant boilerplate code. For each registered C++ reducer, the macro creates an `extern "C"` wrapper function. This wrapper is what's actually exported from the WASM module. It handles:
    1.  Receiving a raw byte buffer from the host.
    2.  Deserializing the sender `Identity` and transaction `Timestamp` from the beginning of this buffer.
    3.  Constructing the `ReducerContext`.
    4.  Deserializing all subsequent user-defined arguments using BSATN.
    5.  Calling your C++ reducer function with the context and deserialized arguments.
    6.  Implementing a `try-catch` block to handle C++ exceptions, log them, and return an appropriate `uint16_t` error code to the host.
*   **SDK Initialization (`_spacetimedb_sdk_init()`):** The SDK requires initialization when the WASM module is loaded by the host. The `<spacetimedb/sdk/spacetimedb_sdk_reducer.h>` header defines and exports an `extern "C" void _spacetimedb_sdk_init()` function. The SpacetimeDB host environment is expected to call this function once upon module load. This function typically sets up any global state required by the SDK, such as the global `Database` instance accessor used by `ReducerContext`.
