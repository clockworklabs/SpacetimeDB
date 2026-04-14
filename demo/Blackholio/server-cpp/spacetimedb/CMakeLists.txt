cmake_minimum_required(VERSION 3.20)
project(spacetime_cpp_module LANGUAGES C CXX)

# Set C++ standard
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Set the module source file
set(MODULE_SOURCE "src/lib.cpp" CACHE STRING "Source file for the SpacetimeDB module")

# Export compile commands for better IDE support
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Set optimization level for Release builds
if(CMAKE_BUILD_TYPE STREQUAL "Release")
  set(CMAKE_CXX_FLAGS_RELEASE "-O2")
endif()

# ------------------------------------------------------------------------------
# SpacetimeDB C++ SDK - Use local SDK from repository
# ------------------------------------------------------------------------------

set(SPACETIMEDB_CPP_SDK_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../../crates/bindings-cpp")
message(STATUS "Using local SpacetimeDB C++ SDK: ${SPACETIMEDB_CPP_SDK_DIR}")
add_subdirectory("${SPACETIMEDB_CPP_SDK_DIR}" "${CMAKE_BINARY_DIR}/_deps/spacetime_cpp_sdk-build")

# Create the main module executable
add_executable(lib ${MODULE_SOURCE})

# Link against the SDK target (preferred alias)
target_link_libraries(lib PRIVATE spacetimedb_cpp_library)

# Configure Emscripten-specific settings for WASM output
if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
  set(EXPORTED_FUNCS "['_malloc','_free','___describe_module__','___call_reducer__','___call_view__','___call_view_anon__','___call_procedure__']")

  # Compile options/defines for WASM
  # (apply to both the module and the SDK, so everything sees the same feature flags)
  target_compile_options(lib PRIVATE -fno-exceptions)
  target_compile_options(spacetimedb_cpp_library PRIVATE -fno-exceptions)

  # Uncomment the following to allow unstable features of SpacetimeDB
  #target_compile_definitions(lib PRIVATE SPACETIMEDB_UNSTABLE_FEATURES)
  #target_compile_definitions(spacetimedb_cpp_library PUBLIC SPACETIMEDB_UNSTABLE_FEATURES)

  target_link_options(lib PRIVATE
    "SHELL:-sSTANDALONE_WASM=1"
    "SHELL:-sWASM=1"
    "SHELL:--no-entry"
    "SHELL:-sEXPORTED_FUNCTIONS=${EXPORTED_FUNCS}"
    "SHELL:-sERROR_ON_UNDEFINED_SYMBOLS=1"
    "SHELL:-sFILESYSTEM=0"
    "SHELL:-sDISABLE_EXCEPTION_CATCHING=1"
    "SHELL:-sALLOW_MEMORY_GROWTH=0"
    "SHELL:-sINITIAL_MEMORY=16MB"
    "SHELL:-sSUPPORT_LONGJMP=0"
    "SHELL:-sSUPPORT_ERRNO=0"
    "SHELL:-std=c++20"
    "SHELL:-O2"
  )

  set_target_properties(lib PROPERTIES OUTPUT_NAME "lib" SUFFIX ".wasm")
endif()
