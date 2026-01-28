cmake_minimum_required(VERSION 3.16)
project(sdk-test-procedure-cpp)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Module source file
if(NOT DEFINED MODULE_SOURCE)
    set(MODULE_SOURCE "src/lib.cpp")
endif()

# Output name
if(NOT DEFINED OUTPUT_NAME)
    set(OUTPUT_NAME "lib")
endif()

# Set the path to the SpacetimeDB C++ library
set(SPACETIMEDB_CPP_LIBRARY_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/bindings-cpp")

# Create the module executable
add_executable(${OUTPUT_NAME} ${MODULE_SOURCE})

# Include directories
target_include_directories(${OUTPUT_NAME} PRIVATE
    ${SPACETIMEDB_CPP_LIBRARY_PATH}/include
)

# Compile options for WASM
if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    target_compile_options(${OUTPUT_NAME} PRIVATE -fno-exceptions)
    # Enable unstable features for transaction support
    target_compile_definitions(${OUTPUT_NAME} PRIVATE SPACETIMEDB_UNSTABLE_FEATURES)
    # Also define for the library so it includes unstable features
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -DSPACETIMEDB_UNSTABLE_FEATURES")
endif()

# Link the SpacetimeDB library
add_subdirectory(${SPACETIMEDB_CPP_LIBRARY_PATH} ${CMAKE_CURRENT_BINARY_DIR}/spacetimedb_cpp_library)
target_link_libraries(${OUTPUT_NAME} PRIVATE spacetimedb_cpp_library)

# Emscripten specific settings
if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
  # Export list including procedure functions
  set(EXPORTED_FUNCS "['_malloc','_free','___describe_module__','___call_reducer__','___call_procedure__']")

  # Produce a standalone .wasm (no JS harness) with no entry point
  target_link_options(lib PRIVATE
    "SHELL:-sSTANDALONE_WASM=1"
    "SHELL:-sWASM=1"
    "SHELL:--no-entry"
    # Exports
    "SHELL:-sEXPORTED_FUNCTIONS=${EXPORTED_FUNCS}"
    # Keep this ON for correctness; you can flip to 0 temporarily to probe
    "SHELL:-sERROR_ON_UNDEFINED_SYMBOLS=1"
    # Trim runtime
    "SHELL:-sFILESYSTEM=0"
    "SHELL:-sDISABLE_EXCEPTION_CATCHING=1"
    "SHELL:-sALLOW_MEMORY_GROWTH=0"
    "SHELL:-sINITIAL_MEMORY=16MB"
    "SHELL:-sSUPPORT_LONGJMP=0"
    "SHELL:-sSUPPORT_ERRNO=0"
    # C++20 and O2 at link step (Emscripten accepts them here too)
    "SHELL:-std=c++20"
    "SHELL:-O2"
  )

  # Name the output lib.wasm
  set_target_properties(lib PROPERTIES OUTPUT_NAME "lib" SUFFIX ".wasm")
endif()
