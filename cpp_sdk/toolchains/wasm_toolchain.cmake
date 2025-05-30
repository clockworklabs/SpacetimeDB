# CMake Toolchain file for Emscripten (WebAssembly)

set(CMAKE_SYSTEM_NAME Emscripten)
set(CMAKE_SYSTEM_PROCESSOR wasm32) # Can be wasm64 if targeting that

# Attempt to find Emscripten SDK
set(EMSDK_ENV $ENV{EMSDK})
if(EMSDK_ENV)
    set(EMSDK_PATH ${EMSDK_ENV})
    message(STATUS "Using EMSDK from environment variable EMSDK: ${EMSDK_PATH}")
elseif($ENV{EMSDK_ROOT})
    set(EMSDK_PATH $ENV{EMSDK_ROOT})
    message(STATUS "Using EMSDK from environment variable EMSDK_ROOT: ${EMSDK_PATH}")
else()
    set(EMSDK_PATH "")
    message(STATUS "EMSDK or EMSDK_ROOT environment variable not set. Assuming emcc/em++ are in PATH.")
endif()

if(NOT CMAKE_C_COMPILER)
    find_program(EMCC_EXECUTABLE emcc PATHS ${EMSDK_PATH} ${EMSDK_PATH}/upstream/emscripten NO_DEFAULT_PATH)
    if(NOT EMCC_EXECUTABLE)
        find_program(EMCC_EXECUTABLE emcc) # Search in PATH if not in EMSDK hint
    endif()
    set(CMAKE_C_COMPILER "${EMCC_EXECUTABLE}" CACHE STRING "C compiler (emcc)" FORCE)
endif()

if(NOT CMAKE_CXX_COMPILER)
    find_program(EMXX_EXECUTABLE em++ PATHS ${EMSDK_PATH} ${EMSDK_PATH}/upstream/emscripten NO_DEFAULT_PATH)
    if(NOT EMXX_EXECUTABLE)
        find_program(EMXX_EXECUTABLE em++) # Search in PATH if not in EMSDK hint
    endif()
    set(CMAKE_CXX_COMPILER "${EMXX_EXECUTABLE}" CACHE STRING "C++ compiler (em++)" FORCE)
endif()

if(NOT CMAKE_AR)
    find_program(EMAR_EXECUTABLE emar PATHS ${EMSDK_PATH} ${EMSDK_PATH}/upstream/emscripten NO_DEFAULT_PATH)
    if(NOT EMAR_EXECUTABLE)
        find_program(EMAR_EXECUTABLE emar) # Search in PATH if not in EMSDK hint
    endif()
    set(CMAKE_AR "${EMAR_EXECUTABLE}" CACHE FILEPATH "Archiver (emar)" FORCE)
endif()


if(NOT CMAKE_C_COMPILER OR NOT CMAKE_CXX_COMPILER)
    message(FATAL_ERROR "Emscripten compilers (emcc/em++) not found. Searched EMSDK path: '${EMSDK_PATH}' and system PATH. Please ensure EMSDK environment variable is set correctly or emcc/em++ are in your PATH.")
else()
    message(STATUS "Using Emscripten C compiler: ${CMAKE_C_COMPILER}")
    message(STATUS "Using Emscripten CXX compiler: ${CMAKE_CXX_COMPILER}")
    if(CMAKE_AR)
        message(STATUS "Using Emscripten archiver: ${CMAKE_AR}")
    else()
        message(WARNING "Emscripten archiver (emar) not found. Static libraries might not build correctly.")
    endif()
endif()

# Set target properties for WASM
set(CMAKE_EXECUTABLE_SUFFIX ".wasm")

# Default compile flags for WASM modules
# -s WASM=1 is default with modern Emscripten when outputting .wasm
# -s SIDE_MODULE=1: Essential for modules that are not the main application (like SpacetimeDB modules).
#                   Prevents Emscripten from generating HTML, main(), etc.
# -s STRICT=1: Enables more checks and helps catch potential issues. Recommended.
# --no-entry: For side modules that don't have a main() entry point in the C/C++ sense.
# -Wall -Wextra: Good general warnings.
set(COMMON_WASM_FLAGS "-O2 -s SIDE_MODULE=1 -s STRICT=1 --no-entry -Wall -Wextra")
set(CMAKE_C_FLAGS_INIT "${COMMON_WASM_FLAGS}" CACHE STRING "Initial C flags for WASM" FORCE)
set(CMAKE_CXX_FLAGS_INIT "${COMMON_WASM_FLAGS}" CACHE STRING "Initial CXX flags for WASM" FORCE)

# Linker flags
# --no-entry is also important for the linker for side modules.
# EXPORT_ALL=1 can be useful for debugging but bloats the module.
# Prefer explicit exports using __attribute__((export_name(...))) or -sEXPORTED_FUNCTIONS=[...]
set(CMAKE_EXE_LINKER_FLAGS_INIT "" CACHE STRING "Initial linker flags for WASM executables" FORCE)
# Example: set(CMAKE_EXE_LINKER_FLAGS_INIT "-s EXPORTED_FUNCTIONS=['_malloc','_free','my_exported_func']")

# Skip rpath handling for Emscripten builds as it's not relevant.
set(CMAKE_SKIP_RPATH TRUE)

# Configure find behavior for cross-compiling.
# This tells CMake to search for headers/libraries primarily in the sysroot provided by Emscripten.
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER) # Don't find host programs
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)  # Find libraries only in target environment
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY) # Find includes only in target environment
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY) # Find packages only in target environment
