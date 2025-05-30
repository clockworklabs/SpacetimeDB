# SpacetimeDB C++ SDK Documentation

## 1. Introduction

### Purpose
The SpacetimeDB C++ SDK allows developers to write server-side application logic (modules) for SpacetimeDB using the C++ programming language. It provides tools and libraries to define data schemas (tables), implement business logic (reducers), interact with the database, and handle data serialization. The compiled output is a WebAssembly (WASM) module that runs within the SpacetimeDB server environment.

### Overview of SDK Features
*   **Table Definition:** Define your data schema using C++ structs or classes and register them with the SDK.
*   **Reducer Implementation:** Write your application logic as C++ functions (reducers) that can be called by clients or other reducers.
*   **Data Serialization:** Automatic and manual data serialization to/from BSATN (Binary SpacetimeDB Abstract Type Notation) for communication with the host and storage.
*   **Database Interaction:** A `Table<T>` API for common database operations like insert, delete, and query.
*   **Build Integration:** CMake-based build system for compiling C++ modules to WebAssembly, with conventions for compatibility with the `spacetime` CLI.
*   **Contextual Information:** Access to transaction context like sender identity and timestamp within reducers.

### Prerequisites
To use the SpacetimeDB C++ SDK, you will need:
*   A C++17 compliant compiler (e.g., Clang, GCC).
*   **Emscripten (emsdk):** Required for compiling C++ to WebAssembly. Ensure `emcc` and `em++` are in your PATH or that the `EMSDK` environment variable is set.
*   **CMake:** Version 3.15 or higher.
*   **SpacetimeDB CLI:** The `spacetime` command-line tool for publishing modules.
*   **Ninja (Recommended):** A fast build system, often used with CMake and Emscripten.

## 2. Getting Started

This section guides you through setting up, building, and publishing your first C++ SpacetimeDB module.

### Project Setup

We recommend the following directory structure for your C++ module:

```
my_s2db_module/
├── CMakeLists.txt            # For building your C++ code
├── Cargo.toml                # For compatibility with `spacetime` CLI
├── toolchains/               # (Optional, can be shared)
│   └── wasm_toolchain.cmake  # CMake toolchain file for Emscripten
└── src/
    ├── my_module.h           # Your C++ header files
    └── my_module.cpp         # Your C++ source files
```

#### 2.1. `Cargo.toml`
Create a `Cargo.toml` file in the root of your module project. This file is primarily for compatibility with the `spacetime` CLI, which uses it to identify your module's name and expected output location.

**Example: `my_s2db_module/Cargo.toml`**
```toml
[package]
name = "my_s2db_module" # This name is important!
version = "0.1.0"
edition = "2021"

# This Cargo.toml file is primarily for compatibility with the `spacetime publish` CLI.
# It helps the CLI identify the project name and expected output path for the pre-compiled WASM module.
# The C++ code itself is built using CMake and a C++ toolchain (e.g., Emscripten).

[lib]
crate-type = ["cdylib"] # Indicates a dynamic library suitable for WASM
```
Replace `"my_s2db_module"` with your actual module name.

#### 2.2. `CMakeLists.txt`
Create a `CMakeLists.txt` file in your module's root directory to manage the C++ build process.

**Key parts:**
*   **Module Name:** Define a variable `MY_MODULE_NAME` that **must match** the `name` in your `Cargo.toml`.
*   **SDK Linking:** Configure CMake to find and link against the SpacetimeDB C++ SDK static library (`libspacetimedb_cpp_sdk_core.a` or similar) and its headers.
*   **WASM Output:** Ensure the compiled WASM file is named `${MY_MODULE_NAME}.wasm` and placed in `target/wasm32-unknown-unknown/release/` relative to your project root.

**Example: `my_s2db_module/CMakeLists.txt`**
```cmake
cmake_minimum_required(VERSION 3.15)
project(MyModuleCMakeProject CXX) # This CMake project name can be different

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Define the module name, this MUST match the 'name' in Cargo.toml
set(MY_MODULE_NAME "my_s2db_module") # Ensure this matches Cargo.toml

# Set the output directory and filename to match Rust's convention
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_SOURCE_DIR}/target/wasm32-unknown-unknown/release)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_SOURCE_DIR}/target/wasm32-unknown-unknown/release)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_SOURCE_DIR}/target/wasm32-unknown-unknown/release)

# Add your module source files (e.g., from the 'src' directory)
add_executable(${MY_MODULE_NAME}
    src/my_module.cpp
    # Add other .cpp files if any
)

# Set the output name of the WASM file explicitly
set_target_properties(${MY_MODULE_NAME} PROPERTIES OUTPUT_NAME "${MY_MODULE_NAME}")
set_target_properties(${MY_MODULE_NAME} PROPERTIES SUFFIX ".wasm")

# --- SpacetimeDB C++ SDK Linking ---
# Adjust these paths based on where your SpacetimeDB C++ SDK is located.
# This example assumes the SDK is two levels up from the current module project.
set(SPACETIMEDB_SDK_DIR ../../sdk CACHE PATH "Path to SpacetimeDB C++ SDK root directory")
set(SPACETIMEDB_SDK_INCLUDE_DIR ${SPACETIMEDB_SDK_DIR}/include)
set(SPACETIMEDB_SDK_BUILD_DIR ${SPACETIMEDB_SDK_DIR}/build) # Assuming SDK is built here
set(SPACETIMEDB_SDK_LIBRARY ${SPACETIMEDB_SDK_BUILD_DIR}/libspacetimedb_cpp_sdk_core.a)

if(NOT EXISTS ${SPACETIMEDB_SDK_INCLUDE_DIR})
    message(FATAL_ERROR "SpacetimeDB SDK include directory not found: ${SPACETIMEDB_SDK_INCLUDE_DIR}")
endif()
if(NOT EXISTS ${SPACETIMEDB_SDK_LIBRARY})
    message(FATAL_ERROR "SpacetimeDB SDK library not found: ${SPACETIMEDB_SDK_LIBRARY}")
endif()

target_include_directories(${MY_MODULE_NAME} PUBLIC 
    ${SPACETIMEDB_SDK_INCLUDE_DIR}
    src # For local headers
)
target_link_libraries(${MY_MODULE_NAME} PUBLIC ${SPACETIMEDB_SDK_LIBRARY})
# --- End SDK Linking ---

# Ensure reducer functions are exported. The SPACETIMEDB_REDUCER macro handles this.
# If issues arise with dead code elimination, you might need explicit exports:
# target_link_options(${MY_MODULE_NAME} PUBLIC "-s EXPORTED_FUNCTIONS=['reducer1_name','reducer2_name','_spacetimedb_sdk_init']")

message(STATUS "Building user module: ${MY_MODULE_NAME}.wasm")
message(STATUS "Output directory: ${CMAKE_RUNTIME_OUTPUT_DIRECTORY}")
```

#### 2.3. `wasm_toolchain.cmake`
This file tells CMake how to use Emscripten to compile for WebAssembly. You can place this in a `toolchains` directory (e.g., `my_s2db_module/../../toolchains/wasm_toolchain.cmake` if shared across projects, or directly in your project).

*(Refer to the `toolchains/wasm_toolchain.cmake` content from Step 6 of the development plan for its full content. Key aspects include setting `CMAKE_SYSTEM_NAME` to `Emscripten`, finding `emcc`/`em++`, and setting compile flags like `-s SIDE_MODULE=1` and `--no-entry`.)*

### Building the Module

1.  **Configure CMake:**
    Navigate to your module's root directory (`my_s2db_module/`) and run:
    ```bash
    cmake -B build -S . -DCMAKE_TOOLCHAIN_FILE=path/to/your/toolchains/wasm_toolchain.cmake
    ```
    Replace `path/to/your/toolchains/wasm_toolchain.cmake` with the actual path.

2.  **Compile:**
    ```bash
    cmake --build build
    ```
    This will produce `${MY_MODULE_NAME}.wasm` in the `my_s2db_module/target/wasm32-unknown-unknown/release/` directory.

#### Example `build_and_publish.sh`
A script can automate these steps.
*(Refer to the `build_and_publish.sh` script from Step 7 of the development plan for a complete example. It includes parsing `Cargo.toml` for the module name, running CMake, building, and then publishing.)*

### Publishing the Module
After successfully building your WASM module, ensure you are in your module's root directory (where `Cargo.toml` is). Then publish using the `spacetime` CLI:

```bash
spacetime publish <module_name_from_cargo_toml> --name <your_db_address_or_alias>
```
For example:
```bash
spacetime publish my_s2db_module --name my_game_database
```
The CLI will use the module name from `Cargo.toml` to find your compiled WASM file at `target/wasm32-unknown-unknown/release/my_s2db_module.wasm` and upload it.

## 3. Core SDK Concepts

### Defining Tables

Tables define the schema of your data in SpacetimeDB.

#### 3.1. C++ Structs/Classes for Tables
You define table rows as C++ structs or classes.

```cpp
// src/my_module.h
#include "bsatn.h" // From SpacetimeDB SDK
#include <string>
#include <cstdint>

namespace my_module_namespace {

struct MyPlayer : public spacetimedb::sdk::bsatn::BsatnSerializable {
    uint64_t player_id;    // Primary Key
    std::string username;
    uint32_t score;

    // Default constructor (optional but often useful)
    MyPlayer() : player_id(0), score(0) {}
    MyPlayer(uint64_t id, std::string name, uint32_t s) 
        : player_id(id), username(std::move(name)), score(s) {}

    // Serialization methods required by BsatnSerializable
    void bsatn_serialize(spacetimedb::sdk::bsatn::bsatn_writer& writer) const override {
        writer.write_u64(player_id);
        writer.write_string(username);
        writer.write_u32(score);
    }

    void bsatn_deserialize(spacetimedb::sdk::bsatn::bsatn_reader& reader) override {
        player_id = reader.read_u64();
        username = reader.read_string();
        score = reader.read_u32();
    }
};

} // namespace my_module_namespace
```

**Requirements:**
*   The struct/class must inherit from `spacetimedb::sdk::bsatn::BsatnSerializable`.
*   It must override `bsatn_serialize` and `bsatn_deserialize` methods. These methods define how your type is converted to and from the BSATN binary format. The order of reads/writes must match.
*   Alternatively, if not inheriting, it must provide public methods `void bsatn_serialize(spacetimedb::sdk::bsatn::bsatn_writer& writer) const` and `void bsatn_deserialize(spacetimedb::sdk::bsatn::bsatn_reader& reader)`.

#### 3.2. Registering Tables
Use the `SPACETIMEDB_REGISTER_TABLE` macro (from `spacetimedb_sdk_table_registry.h`) in one of your `.cpp` files to register your table type with the SDK.

```cpp
// src/my_module.cpp
#include "my_module.h" // Where MyPlayer is defined
#include "spacetimedb_sdk_table_registry.h" // SDK header

// Register the MyPlayer table
SPACETIMEDB_REGISTER_TABLE(my_module_namespace::MyPlayer, "players_table", "player_id");
```
*   **`CppStructType`**: The C++ type for the table row (e.g., `my_module_namespace::MyPlayer`).
*   **`"db_table_name"`**: The string name this table will have in the database (e.g., `"players_table"`).
*   **`"pk_field_name"`**: The string name of the field in your C++ struct that acts as the primary key (e.g., `"player_id"`).
    *   **Important Assumption:** The SDK currently assumes the primary key field specified here is the **first field** serialized in your `bsatn_serialize` method. This means its column index for operations like `delete_by_col_eq` will be `0`.

This macro registers metadata about your table, allowing the SDK to associate the C++ type with the database table name and understand its primary key.

### Writing Reducers

Reducers are C++ functions that implement your application's business logic and transactions.

#### 3.2.1. Reducer Function Signature
Reducers are ordinary C++ functions with a specific signature:
```cpp
// src/my_module.cpp
#include "my_module.h" // For MyPlayer, etc.
#include "reducer_context.h" // For spacetimedb::sdk::ReducerContext
#include "spacetimedb_abi.h" // For _console_log if used directly

void add_score(spacetimedb::sdk::ReducerContext& ctx, uint64_t target_player_id, uint32_t points_to_add) {
    // ... reducer logic ...
    std::string message = "Reducer add_score called for player " + std::to_string(target_player_id);
    _console_log(1 /*INFO*/, nullptr, 0, reinterpret_cast<const uint8_t*>(message.c_str()), message.length());
}
```
*   The first argument **must** be `spacetimedb::sdk::ReducerContext& ctx`.
*   Subsequent arguments must be types supported by BSATN (primitives, `std::string`, `std::vector<uint8_t>`, or other `BsatnSerializable` types).

#### 3.2.2. Registering Reducers
Use the `SPACETIMEDB_REDUCER` or `SPACETIMEDB_REDUCER_NO_ARGS` macros (from `spacetimedb_sdk_reducer.h`) in a `.cpp` file to make your C++ function callable as a reducer.

```cpp
// src/my_module.cpp
#include "spacetimedb_sdk_reducer.h" // SDK header for reducer macros

// ... (definition of add_score, MyPlayer etc.) ...

SPACETIMEDB_REDUCER(my_module_namespace::add_score, uint64_t, uint32_t);

// Example for a reducer with no arguments other than context
namespace my_module_namespace {
void initialize_game_state(spacetimedb::sdk::ReducerContext& ctx) {
    // ... logic ...
    std::string message = "initialize_game_state called.";
    _console_log(1, nullptr, 0, reinterpret_cast<const uint8_t*>(message.c_str()), message.length());
}
}
SPACETIMEDB_REDUCER_NO_ARGS(my_module_namespace::initialize_game_state);
```
*   The first argument to the macro is the fully qualified C++ function name.
*   Subsequent arguments are the C++ types of the reducer function's parameters (excluding `ReducerContext`).
*   The macro generates a WASM export with the same name as your C++ function (e.g., `add_score`).
*   **Argument Passing:** The SpacetimeDB host encodes all arguments (including sender `Identity` and transaction `Timestamp` first, followed by user arguments) into a single BSATN buffer. The generated wrapper deserializes these.
*   **Error Handling:** If your C++ reducer throws an exception, the wrapper generated by the macro will catch it, log an error message using `_console_log`, and return a non-zero error code (`uint16_t`) to the host. Otherwise, it returns `0` for success.

### `ReducerContext` Usage
The `ReducerContext` object (`ctx`) passed to your reducers provides access to transaction information and database operations.

*   **`ctx.get_sender()`**: Returns a `const spacetimedb::sdk::Identity&` representing the identity of the client or principal that invoked the reducer.
*   **`ctx.get_timestamp()`**: Returns a `spacetimedb::sdk::Timestamp` object for the current transaction.
*   **`ctx.db()`**: Returns a reference to a `spacetimedb::sdk::Database` object, which is your entry point for database operations.

### Database Operations (`Database` and `Table<T>`)

#### 3.4.1. Getting a Table Instance
```cpp
void my_logic(spacetimedb::sdk::ReducerContext& ctx) {
    // Assumes MyPlayer and "players_table" were registered
    auto player_table = ctx.db().get_table<my_module_namespace::MyPlayer>("players_table");
    // player_table is now an instance of spacetimedb::sdk::Table<my_module_namespace::MyPlayer>
}
```
The `get_table<MyRowType>("db_table_name")` method uses the registered table metadata to associate the C++ type `MyRowType` with the database table name.

#### 3.4.2. Inserting Rows
```cpp
my_module_namespace::MyPlayer new_player(1, "player_one", 100);
player_table.insert(new_player); 
// After this call, if 'player_id' were an auto-incrementing PK filled by the DB,
// new_player.player_id would be updated. (This depends on ABI and host behavior)
```
The `insert` method serializes the `row_object` using its `bsatn_serialize` method and calls the underlying `_insert` ABI function. The object passed to `insert` is non-const because the host might modify the buffer (e.g., to set a primary key), and the SDK deserializes this modification back into the object.

#### 3.4.3. Deleting Rows
```cpp
// Delete player where player_id (column 0) equals 123
uint32_t deleted_rows = player_table.delete_by_col_eq(0, static_cast<uint64_t>(123));
```
*   `col_idx`: The 0-based index of the column to match against. For primary keys registered with `SPACETIMEDB_REGISTER_TABLE`, this is currently assumed to be `0`.
*   `ValueType`: The value to match. Must be BSATN-serializable.

#### 3.4.4. Iterating Over a Table (Full Scan)
```cpp
for (const auto& player : player_table.iter()) {
    // Process player (player.player_id, player.username, etc.)
    std::string msg = "Found player: " + player.username;
     _console_log(1, nullptr, 0, reinterpret_cast<const uint8_t*>(msg.c_str()), msg.length());
}
```
`player_table.iter()` returns a `TableIterator<MyPlayer>`. The iterator handles deserialization of rows.

#### 3.4.5. Finding Rows by Column Value
```cpp
std::string search_username = "player_one";
// Assume 'username' is column index 1 (0 is player_id)
// This requires knowing the column index for 'username'.
// A more advanced SDK might get this from metadata. For now, assume it's known.
// uint32_t username_col_idx = 1; 
// std::vector<my_module_namespace::MyPlayer> found_players = player_table.find_by_col_eq(username_col_idx, search_username);

// If searching by PK (column 0):
uint64_t target_player_id = 1;
std::vector<my_module_namespace::MyPlayer> found_players = player_table.find_by_col_eq(0, target_player_id);

for (const auto& player : found_players) {
    // process player
}
```
`find_by_col_eq` returns a `std::vector<T>` containing all matching rows. The `ValueType` must be BSATN-serializable.

### Logging
Use the `_console_log` ABI function for logging from your C++ module. It's declared in `spacetimedb_abi.h`.

```cpp
// From spacetimedb_quickstart::kv_store.h or similar
// const uint8_t LOG_LEVEL_INFO = 1; 

std::string message = "This is a log message.";
_console_log(LOG_LEVEL_INFO, // level
             nullptr, 0,      // target_ptr, target_len (optional)
             reinterpret_cast<const uint8_t*>(message.c_str()), message.length());
```
A helper function within your module namespace can make this more convenient.

### Supported Data Types
The C++ SDK directly supports serialization/deserialization for:
*   **Primitives:** `bool`, `uint8_t`, `uint16_t`, `uint32_t`, `uint64_t`, `int8_t`, `int16_t`, `int32_t`, `int64_t`, `float`, `double`.
*   **Strings:** `std::string` (encoded as UTF-8).
*   **Byte Arrays:** `std::vector<uint8_t>`.
*   **Collections:** `std::vector<T>` where `T` is any other supported BSATN-serializable type (including primitives, strings, or custom structs).
*   **SDK Types:** `spacetimedb::sdk::Identity`, `spacetimedb::sdk::Timestamp`.
*   **Custom Types:** Any C++ struct/class that implements the `BsatnSerializable` interface or provides the `bsatn_serialize`/`bsatn_deserialize` methods.

## 4. KeyValueStore Example Walkthrough

The `quickstart_cpp_kv` example module demonstrates a simple key-value store.

*(Refer to the files generated in Step 8: `quickstart_cpp_kv/Cargo.toml`, `quickstart_cpp_kv/CMakeLists.txt`, `quickstart_cpp_kv/src/kv_store.h`, `quickstart_cpp_kv/src/kv_store.cpp` for the full code.)*

**Key Code Sections:**

*   **`kv_store.h`:**
    *   Defines the `KeyValue` struct (inheriting `BsatnSerializable`) with `key_str` and `value_str`.
    *   Declares reducer functions: `kv_put`, `kv_get`, `kv_del`.
*   **`kv_store.cpp`:**
    *   Implements `KeyValue::bsatn_serialize` and `KeyValue::bsatn_deserialize`.
    *   Registers the table: `SPACETIMEDB_REGISTER_TABLE(spacetimedb_quickstart::KeyValue, "kv_pairs", "key_str");`
    *   Implements reducer functions:
        *   `kv_put`: Uses `ctx.db().get_table<KeyValue>("kv_pairs")`, then `find_by_col_eq` and `delete_by_col_eq` to simulate an upsert, followed by `insert`.
        *   `kv_get`: Uses `find_by_col_eq` to retrieve and log a value.
        *   `kv_del`: Uses `delete_by_col_eq` to remove an entry.
    *   Registers reducers: `SPACETIMEDB_REDUCER(spacetimedb_quickstart::kv_put, const std::string&, const std::string&);`, etc.

**Build and Publish:**
1.  Navigate to the `quickstart_cpp_kv` directory.
2.  Ensure your `toolchains/wasm_toolchain.cmake` path is correct in `CMakeLists.txt` or when you run CMake.
3.  Configure: `cmake -B build -S . -DCMAKE_TOOLCHAIN_FILE=../../toolchains/wasm_toolchain.cmake` (adjust path if needed).
4.  Build: `cmake --build build`.
    This creates `quickstart_cpp_kv/target/wasm32-unknown-unknown/release/kvstore_module.wasm`.
5.  Publish (from `quickstart_cpp_kv` directory):
    ```bash
    spacetime publish kvstore_module --name my_kv_database 
    ```

**Example `spacetime call` commands:**
```bash
spacetime call my_kv_database kv_put --key "greeting" --value "Hello C++ SDK!"
spacetime call my_kv_database kv_get --key "greeting"
spacetime call my_kv_database kv_del --key "greeting"
spacetime call my_kv_database kv_get --key "greeting" 
```
*(Note: The exact format for `--key <value>` depends on how the `spacetime` CLI handles named arguments for BSATN. BSATN itself is positional for struct fields. The CLI might map these named arguments to a BSATN struct representation before sending to the reducer).*

## 5. Advanced Topics (Brief Mention)

*   **C ABI (`spacetimedb_abi.h`):** The SDK interacts with the SpacetimeDB host environment through a low-level C ABI. This ABI defines functions for logging, buffer management, database operations, and accessing transaction context. The C++ SDK provides safer, more idiomatic wrappers around this ABI.
*   **BSATN Serialization (`bsatn.h`, `bsatn.cpp`):** Data passed between the WASM module and the host, as well as data stored in tables, is serialized using BSATN. The SDK provides `bsatn_writer` and `bsatn_reader` classes for this. User-defined types must be made BSATN-serializable.
*   **Reducer Macro Internals:** The `SPACETIMEDB_REDUCER` macros generate `extern "C"` wrapper functions that are exported from the WASM module. These wrappers handle:
    *   Deserializing sender identity and transaction timestamp from the start of the incoming argument buffer.
    *   Creating the `ReducerContext`.
    *   Deserializing user arguments from the BSATN buffer.
    *   Calling the user's C++ reducer function.
    *   Catching C++ exceptions and translating them to error codes for the host.
*   **SDK Initialization (`_spacetimedb_sdk_init()`):** The SDK provides an `_spacetimedb_sdk_init()` function (exported from WASM, typically found in `spacetimedb_sdk_reducer.h`). The SpacetimeDB host environment is expected to call this function when the WASM module is loaded to perform any necessary SDK setup (like initializing the global database accessor for reducers).
```
