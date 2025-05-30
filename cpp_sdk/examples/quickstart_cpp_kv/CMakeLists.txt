cmake_minimum_required(VERSION 3.15)
project(KvStoreModule CXX) # CMake project name for this example

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Define the module name, this MUST match the 'name' in Cargo.toml
set(MODULE_NAME "kvstore_module")

# Set the output directory and filename to match Rust's convention
# Output directory relative to the current source directory (examples/quickstart_cpp_kv)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/target/wasm32-unknown-unknown/release)
# Ensure CMake creates this directory if it doesn't exist when creating the target (usually does)

# Add module source files
add_executable(${MODULE_NAME}
    src/kv_store.cpp
)

# Set the output name and suffix for the WASM file
set_target_properties(${MODULE_NAME} PROPERTIES OUTPUT_NAME "${MODULE_NAME}")
set_target_properties(${MODULE_NAME} PROPERTIES SUFFIX ".wasm")

# --- SpacetimeDB C++ SDK Linking ---
# This section assumes the SpacetimeDB C++ SDK is either:
# 1. Installed system-wide or in a known location (use find_package - preferred for robust builds).
# 2. Built as part of a parent CMake project (use target_link_libraries with the SDK's target name).
# 3. Located at a known relative path (simpler for bundled examples).

# Option 3: Using relative paths
# Path to the SDK directory, relative to this CMakeLists.txt (which is in examples/quickstart_cpp_kv)
# ../../sdk correctly navigates from cpp_sdk/examples/quickstart_cpp_kv/ to cpp_sdk/sdk/
set(SPACETIMEDB_SDK_DIR_REL ../../sdk)
get_filename_component(SPACETIMEDB_SDK_DIR ${SPACETIMEDB_SDK_DIR_REL} ABSOLUTE CACHE PATH "Absolute path to SpacetimeDB C++ SDK root directory")

# Check if the SDK directory exists
if(NOT IS_DIRECTORY ${SPACETIMEDB_SDK_DIR})
    message(FATAL_ERROR "SpacetimeDB SDK directory not found. Calculated absolute path: ${SPACETIMEDB_SDK_DIR}. Please ensure the relative path '${SPACETIMEDB_SDK_DIR_REL}' is correct.")
endif()

# Add the SDK as a subdirectory. This builds the SDK if not already built,
# and makes its targets available.
# The 'build_sdk' is the build directory for the SDK within this project's build tree.
# The 'EXCLUDE_FROM_ALL' means 'make' or 'cmake --build' won't build the SDK unless this target depends on it.
add_subdirectory(${SPACETIMEDB_SDK_DIR} ${CMAKE_BINARY_DIR}/sdk_build EXCLUDE_FROM_ALL)

# Link against the public alias target from the SDK's CMakeLists.txt
target_link_libraries(${MODULE_NAME} PUBLIC spacetimedb::sdk::spacetimedb_cpp_sdk) # Using namespaced target if SDK exports it like this
# If the SDK alias is just spacetimedb_cpp_sdk:
# target_link_libraries(${MODULE_NAME} PUBLIC spacetimedb_cpp_sdk)

# Include directories from the SDK are automatically handled if the SDK's
# target_include_directories for its interface library (SpacetimeDBCppSDK_Headers)
# uses INTERFACE or PUBLIC. The spacetimedb_cpp_sdk target should propagate this.

# Local include for this module's headers (e.g., src/kv_store.h)
target_include_directories(${MODULE_NAME} PUBLIC src)
# --- End SDK Linking ---


# Ensure the reducer functions exported by SPACETIMEDB_REDUCER are kept.
# The __attribute__((export_name(...))) in the reducer macro should handle this.
# Emscripten's default dead code elimination (DCE) can sometimes be aggressive.
# If reducers are stripped, explicitly list them or use EXPORT_ALL.
# For SpacetimeDB, it's critical that _spacetimedb_sdk_init and all reducer functions are exported.
# Example for explicit export (preferred over EXPORT_ALL for size):
# list(APPEND WASM_EXPORTED_FUNCTIONS "'_spacetimedb_sdk_init'") # SDK init function
# list(APPEND WASM_EXPORTED_FUNCTIONS "'kv_put'") # Add your reducer names here
# list(APPEND WASM_EXPORTED_FUNCTIONS "'kv_get'")
# list(APPEND WASM_EXPORTED_FUNCTIONS "'kv_del'")
# string(JOIN "," LINKER_EXPORT_FLAG_CONTENT "${WASM_EXPORTED_FUNCTIONS}")
# target_link_options(${MODULE_NAME} PUBLIC "-sEXPORTED_FUNCTIONS=[${LINKER_EXPORT_FLAG_CONTENT}]")
# For simplicity now, relying on __attribute__((export_name)) and possibly SIDE_MODULE behavior.

message(STATUS "Building user module: ${MODULE_NAME}.wasm")
message(STATUS "Output directory: ${CMAKE_RUNTIME_OUTPUT_DIRECTORY}")
message(STATUS "To build, ensure you are in your project's build directory and run:")
message(STATUS "cmake -DCMAKE_TOOLCHAIN_FILE=<path_to_toolchains_dir>/wasm_toolchain.cmake <path_to_module_source_dir>")
message(STATUS "Then: cmake --build .")
message(STATUS "Example configuration from 'examples/quickstart_cpp_kv/build':")
message(STATUS "cmake -DCMAKE_TOOLCHAIN_FILE=../../toolchains/wasm_toolchain.cmake ..")
message(STATUS "cmake --build .")
