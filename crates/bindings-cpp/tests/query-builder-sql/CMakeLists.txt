cmake_minimum_required(VERSION 3.16)
project(query_builder_sql_tests LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(query_builder_sql_tests
    main.cpp
    query_builder_sql_tests.cpp
)

target_include_directories(query_builder_sql_tests PRIVATE
    ../../include
)

if(CMAKE_CXX_COMPILER_ID STREQUAL "GNU")
    target_compile_options(query_builder_sql_tests PRIVATE -Wno-format-truncation)
endif()
