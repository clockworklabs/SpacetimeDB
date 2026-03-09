# SpacetimeDB C++ Module Library CMake Configuration

cmake_minimum_required(VERSION 3.15)
project(SpacetimeDBCppModuleLibrary 
    VERSION 1.12.0
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

# ---- Tests ----
# Default: ON only when building this project directly; OFF when used via FetchContent/add_subdirectory
if(CMAKE_VERSION VERSION_LESS 3.21)
    # Fallback heuristic for older CMake
    set(_is_top_level FALSE)
    if(CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
        set(_is_top_level TRUE)
    endif()
else()
    set(_is_top_level ${PROJECT_IS_TOP_LEVEL})
endif()

option(BUILD_TESTS "Build the test suite" ${_is_top_level})

if(BUILD_TESTS AND NOT CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    enable_testing()
    
    # Add test executable
    add_executable(test_bsatn tests/main.cpp tests/module_library_unit_tests.cpp)
    
    # Link against the module library
    target_link_libraries(test_bsatn PRIVATE spacetimedb_cpp_library)
    
    # Set C++20 standard for tests
    target_compile_features(test_bsatn PRIVATE cxx_std_20)
    
    # Add test to CTest
    add_test(NAME bsatn_tests COMMAND test_bsatn)
    
    # Add verbose test variant
    add_test(NAME bsatn_tests_verbose COMMAND test_bsatn -v)
    
    # Set test properties
    set_tests_properties(bsatn_tests PROPERTIES 
        TIMEOUT 30
        LABELS "unit"
    )
    
    set_tests_properties(bsatn_tests_verbose PROPERTIES 
        TIMEOUT 30
        LABELS "unit;verbose"
    )
endif()
