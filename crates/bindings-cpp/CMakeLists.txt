# SpacetimeDB C++ Module Library CMake Configuration

cmake_minimum_required(VERSION 3.15)
project(SpacetimeDBCppModuleLibrary 
    VERSION 2.2.0
    LANGUAGES CXX)

# Generate version header from template
configure_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/include/spacetimedb/version.h.in
    ${CMAKE_CURRENT_BINARY_DIR}/include/spacetimedb/version.h
    @ONLY
)

# Module Library source files
set(LIBRARY_SOURCES
    src/abi/module_exports.cpp
    src/abi/wasi_shims.cpp
    src/internal/Module.cpp
    src/internal/AlgebraicType.cpp  # Required for V9 autogen types
    src/internal/v9_builder.cpp  # V9 incremental module builder
    src/internal/v10_builder.cpp  # V10 facade over module definition assembly
    src/internal/module_type_registration.cpp  # Unified type registration system
)

add_library(spacetimedb_cpp_library STATIC)
add_library(spacetimedb::spacetimedb_cpp_library ALIAS spacetimedb_cpp_library)

target_sources(spacetimedb_cpp_library PRIVATE ${LIBRARY_SOURCES})

# Require C++20 for consumers of this library without forcing global flags
target_compile_features(spacetimedb_cpp_library PUBLIC cxx_std_20)
target_compile_definitions(spacetimedb_cpp_library PRIVATE SPACETIMEDB_UNSTABLE_FEATURES)

# Set include directories
target_include_directories(spacetimedb_cpp_library
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/include>
        $<INSTALL_INTERFACE:include>
)

# Create an alias target for better namespacing
add_library(spacetimedb::spacetimedb_cpp_library ALIAS spacetimedb_cpp_library)

# Set compile options if building for WASM
if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    target_compile_options(spacetimedb_cpp_library PRIVATE 
        -O2                    # Optimize for performance
        -fno-exceptions        # Disable exceptions for WASM compatibility
        -ffunction-sections    # Place each function in its own section
        -fdata-sections        # Place each data item in its own section
        -Wall -Wextra          # Enable warnings
    )
    # Note: -g0 should be set in the final executable's linker flags, not here
    # This allows debugging the library during development if needed
endif()

# Export compile commands for better IDE support (top-level only)
if(PROJECT_IS_TOP_LEVEL)
    set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
endif()

# Unit/compile/smoke test harnesses live under `tests/` as standalone runners
# rather than being built through the top-level library CMake target.
