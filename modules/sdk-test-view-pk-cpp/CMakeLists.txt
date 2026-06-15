cmake_minimum_required(VERSION 3.16)
project(sdk-test-view-pk-cpp)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT DEFINED MODULE_SOURCE)
    set(MODULE_SOURCE "src/lib.cpp")
endif()

if(NOT DEFINED OUTPUT_NAME)
    set(OUTPUT_NAME "lib")
endif()

set(SPACETIMEDB_CPP_LIBRARY_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/bindings-cpp")

add_executable(${OUTPUT_NAME} ${MODULE_SOURCE})

target_include_directories(${OUTPUT_NAME} PRIVATE
    ${SPACETIMEDB_CPP_LIBRARY_PATH}/include
)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    target_compile_options(${OUTPUT_NAME} PRIVATE -fno-exceptions -O2 -g0)
endif()

add_subdirectory(${SPACETIMEDB_CPP_LIBRARY_PATH} ${CMAKE_CURRENT_BINARY_DIR}/spacetimedb_cpp_library)
target_link_libraries(${OUTPUT_NAME} PRIVATE spacetimedb_cpp_library)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
  set(EXPORTED_FUNCS "['_malloc','_free','___describe_module__','___call_reducer__','___call_view__','___call_view_anon__']")

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
