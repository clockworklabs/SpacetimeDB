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
    environment_unit_tests.cpp
    hosted_auth_unit_tests.cpp
    function_visibility_unit_tests.cpp
)

# Exercise the real module builder without the standalone WASI shims, which
# replace the Node test runner's standard I/O and process lifecycle functions.
target_sources(bindings_cpp_unit_tests PRIVATE
    ../../src/internal/Module.cpp
    ../../src/internal/AlgebraicType.cpp
    ../../src/internal/v9_builder.cpp
    ../../src/internal/v10_builder.cpp
    ../../src/internal/module_type_registration.cpp
)

add_executable(hosted_auth_unit_tests main.cpp hosted_auth_unit_tests.cpp)

foreach(test_target bindings_cpp_unit_tests hosted_auth_unit_tests)
target_include_directories(${test_target} PRIVATE
    ../../include
)

target_compile_definitions(${test_target} PRIVATE
    SPACETIMEDB_UNSTABLE_FEATURES
)

if(MSVC)
    target_compile_options(${test_target} PRIVATE /W4)
else()
    target_compile_options(${test_target} PRIVATE -Wall -Wextra)
endif()

target_link_options(${test_target} PRIVATE
    "SHELL:-sWASM=1"
    "SHELL:-sENVIRONMENT=node"
    "SHELL:-sEXIT_RUNTIME=1"
    "SHELL:-sASSERTIONS=1"
    "SHELL:-O2"
)

set_target_properties(${test_target} PROPERTIES SUFFIX ".cjs")
endforeach()
