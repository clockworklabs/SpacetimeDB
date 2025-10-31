# SpacetimeDB C++ Module Library CMake Configuration

cmake_minimum_required(VERSION 3.15)
project(SpacetimeDBCppModuleLibrary CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Module Library source files
set(LIBRARY_SOURCES
    src/abi/module_exports.cpp
    src/abi/wasi_shims.cpp
    src/internal/Module.cpp
    src/internal/AlgebraicType.cpp  # Required for V9 autogen types
    src/internal/v9_builder.cpp  # V9 incremental module builder
    src/internal/v9_type_registration.cpp  # Unified type registration system
    # src/library/index_management.cpp  # Disabled due to conflicts with index_ops.h
    # src/type_registry.cpp  # REMOVED - TypeRegistry system eliminated
)

# Create the Module Library
add_library(spacetimedb_cpp_library STATIC ${LIBRARY_SOURCES})

# Set include directories
target_include_directories(spacetimedb_cpp_library
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
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

# Export compile commands for better IDE support
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Testing configuration
option(BUILD_TESTS "Build the test suite" ON)

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