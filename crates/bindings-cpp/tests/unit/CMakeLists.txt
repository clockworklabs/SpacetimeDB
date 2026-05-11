cmake_minimum_required(VERSION 3.16)
project(bindings_cpp_unit_tests LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
    message(FATAL_ERROR "tests/unit is intended to be built with Emscripten via emcmake")
endif()

add_executable(bindings_cpp_unit_tests
    main.cpp
    http_unit_tests.cpp
)

target_include_directories(bindings_cpp_unit_tests PRIVATE
    ../../include
)

if(MSVC)
    target_compile_options(bindings_cpp_unit_tests PRIVATE /W4)
else()
    target_compile_options(bindings_cpp_unit_tests PRIVATE -Wall -Wextra)
endif()

target_link_options(bindings_cpp_unit_tests PRIVATE
    "SHELL:-sWASM=1"
    "SHELL:-sENVIRONMENT=node"
    "SHELL:-sEXIT_RUNTIME=1"
    "SHELL:-sASSERTIONS=1"
    "SHELL:-O2"
)

set_target_properties(bindings_cpp_unit_tests PROPERTIES SUFFIX ".cjs")
