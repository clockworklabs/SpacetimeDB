cmake_minimum_required(VERSION 3.20)
project(spacetime_cpp_module LANGUAGES C CXX)

# ------------------------------------------------------------------------------
# SpacetimeDB C++ Bindings Configuration
#
# Default: Fetches from GitHub using SPACETIMEDB_CPP_VERSION
#   - MAJOR.MINOR (e.g., "1.12") → fetches release/MAJOR.MINOR branch, gets latest patch
#   - MAJOR.MINOR.PATCH (e.g., "1.12.0") → fetches vMAJOR.MINOR.PATCH tag, exact version
#
# Advanced options:
#   - SPACETIMEDB_CPP_REF: Override Git ref directly 
#     (e.g., "release/latest", "main", "v1.12.0")
#
#   - SPACETIMEDB_CPP_DIR: Use local bindings clone (for bindings development)
#     PowerShell: $env:SPACETIMEDB_CPP_DIR="E:\SpacetimeDB\crates\bindings-cpp"
#     Bash:       export SPACETIMEDB_CPP_DIR=/path/to/SpacetimeDB/crates/bindings-cpp
#     The directory should contain the bindings' CMakeLists.txt at its root.
# ------------------------------------------------------------------------------

set(SPACETIMEDB_CPP_VERSION "1.12.0" CACHE STRING "Version selector: MAJOR.MINOR (uses release/MAJOR.MINOR) or MAJOR.MINOR.PATCH (uses tag vMAJOR.MINOR.PATCH)")
set(SPACETIMEDB_CPP_REF "" CACHE STRING "Override Git ref directly (e.g. release/1.0, release/latest, v1.0.0)")  
set(SPACETIMEDB_CPP_DIR "" CACHE PATH "Path to a local clone of SpacetimeDB C++ bindings (overrides FetchContent)")

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

if(NOT SPACETIMEDB_CPP_DIR AND DEFINED ENV{SPACETIMEDB_CPP_DIR})
  set(SPACETIMEDB_CPP_DIR "$ENV{SPACETIMEDB_CPP_DIR}")
endif()

if (SPACETIMEDB_CPP_REF STREQUAL "")
  if (SPACETIMEDB_CPP_VERSION MATCHES "^[0-9]+\\.[0-9]+\\.[0-9]+$")
    set(SPACETIMEDB_CPP_REF "v${SPACETIMEDB_CPP_VERSION}")
  elseif (SPACETIMEDB_CPP_VERSION MATCHES "^[0-9]+\\.[0-9]+$")
    set(SPACETIMEDB_CPP_REF "release/${SPACETIMEDB_CPP_VERSION}")
  else()
    message(FATAL_ERROR "SPACETIMEDB_CPP_VERSION must be MAJOR.MINOR or MAJOR.MINOR.PATCH")
  endif()
endif()

if(SPACETIMEDB_CPP_DIR)
  message(STATUS "Using local SpacetimeDB C++ bindings: ${SPACETIMEDB_CPP_DIR}")
  add_subdirectory("${SPACETIMEDB_CPP_DIR}" "${CMAKE_BINARY_DIR}/_deps/spacetimedb_cpp-build")
else()
  include(FetchContent)

  FetchContent_Declare(
    spacetimedb_cpp_sdk
    GIT_REPOSITORY https://github.com/clockworklabs/spacetimedb-bindings-cpp.git
    GIT_TAG ${SPACETIMEDB_CPP_REF}
  )
  FetchContent_MakeAvailable(spacetimedb_cpp_sdk)
endif()

# Create the main module executable
add_executable(lib ${MODULE_SOURCE})

# Link against the bindings target (preferred alias)
target_link_libraries(lib PRIVATE spacetimedb_cpp_library)

# Configure Emscripten-specific settings for WASM output
if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
  set(EXPORTED_FUNCS "['_malloc','_free','___describe_module__','___call_reducer__','___call_view__','___call_view_anon__','___call_procedure__']")

  # Compile options/defines for WASM
  # (apply to both the module and the bindings, so everything sees the same feature flags)
  target_compile_options(lib PRIVATE -fno-exceptions -O2 -g0)
  target_compile_options(spacetimedb_cpp_library PRIVATE -fno-exceptions -O2 -g0)

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
    "SHELL:-g0"
  )

  set_target_properties(lib PROPERTIES OUTPUT_NAME "lib" SUFFIX ".wasm")
endif()
