cmake_minimum_required(VERSION 3.16)
project(type-isolation-tests)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Build the SpacetimeDB library once
set(SPACETIMEDB_CPP_LIBRARY_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../..")

# Build the library as a static library
add_subdirectory(${SPACETIMEDB_CPP_LIBRARY_PATH} ${CMAKE_CURRENT_BINARY_DIR}/spacetimedb_lib)

# Export the library path for modules to use
set(SPACETIMEDB_LIBRARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/spacetimedb_lib CACHE PATH "Path to built SpacetimeDB library")
set(SPACETIMEDB_INCLUDE_DIR ${SPACETIMEDB_CPP_LIBRARY_PATH}/include CACHE PATH "Path to SpacetimeDB includes")