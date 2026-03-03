#pragma once

#include <spacetimedb.h>
#include <cstdint>

using namespace SpacetimeDB;

// Black box function to prevent compiler optimizations during benchmarking
template<typename T>
inline void black_box(const T& value) {
    // Use volatile to prevent the compiler from optimizing away the value
    volatile const void* ptr = &value;
    (void)ptr;
}

// Load configuration struct - defines test data sizes for benchmarks
struct Load {
    uint32_t initial_load;
    uint32_t small_table;
    uint32_t num_players;
    uint32_t big_table;
    uint32_t biggest_table;

    // Default constructor required by SPACETIMEDB_STRUCT
    Load() = default;

    Load(uint32_t initial_load_param)
        : initial_load(initial_load_param)
        , small_table(initial_load_param)
        , num_players(initial_load_param)
        , big_table(initial_load_param * 50)
        , biggest_table(initial_load_param * 100)
    {}
};
SPACETIMEDB_STRUCT(Load, initial_load, small_table, num_players, big_table, biggest_table)