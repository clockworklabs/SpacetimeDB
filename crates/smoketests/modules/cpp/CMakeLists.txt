cmake_minimum_required(VERSION 3.16)
project(smoketest_cpp_module)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(SPACETIMEDB_CPP_LIBRARY_PATH "@SPACETIMEDB_CPP_LIBRARY_PATH@")

add_executable(lib src/lib.cpp)

target_include_directories(lib PRIVATE
    ${SPACETIMEDB_CPP_LIBRARY_PATH}/include
)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    target_compile_options(lib PRIVATE -fno-exceptions -O2 -g0)
    target_compile_definitions(lib PRIVATE SPACETIMEDB_UNSTABLE_FEATURES)
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -DSPACETIMEDB_UNSTABLE_FEATURES")
endif()

add_subdirectory(${SPACETIMEDB_CPP_LIBRARY_PATH} ${CMAKE_CURRENT_BINARY_DIR}/spacetimedb_cpp_library)
target_link_libraries(lib PRIVATE spacetimedb_cpp_library)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    set(EXPORTED_FUNCS
        "['_malloc','_free','___describe_module__','___call_reducer__','___call_procedure__','___call_http_handler__']"
    )

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
