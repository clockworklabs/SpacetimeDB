//! STDB module used for benchmarks.
//! 
//! This file provides pure database operations with various indexing strategies for performance testing.
//! 
//! We instantiate multiple copies of each table with different indexing:
//! - unique_0_*: single unique key on first field  
//! - no_index_*: no indexes at all
//! - btree_each_column_*: btree index on every column

#include "common.h"
#include <string>
#include <vector>

// =============================================================================
// SYNTHETIC BENCHMARK - TABLE VARIANTS FOR u32_u64_str (id, age, name)
// =============================================================================

// Unique indexed version
struct unique_0_u32_u64_str_t {
    uint32_t id;
    uint64_t age;
    std::string name;
};
SPACETIMEDB_STRUCT(unique_0_u32_u64_str_t, id, age, name)
SPACETIMEDB_TABLE(unique_0_u32_u64_str_t, unique_0_u32_u64_str, Public)
FIELD_Unique(unique_0_u32_u64_str, id)

// No index version
struct no_index_u32_u64_str_t {
    uint32_t id;
    uint64_t age;
    std::string name;
};
SPACETIMEDB_STRUCT(no_index_u32_u64_str_t, id, age, name)
SPACETIMEDB_TABLE(no_index_u32_u64_str_t, no_index_u32_u64_str, Public)

// BTree index on each column version
struct btree_each_column_u32_u64_str_t {
    uint32_t id;
    uint64_t age;
    std::string name;
};
SPACETIMEDB_STRUCT(btree_each_column_u32_u64_str_t, id, age, name)
SPACETIMEDB_TABLE(btree_each_column_u32_u64_str_t, btree_each_column_u32_u64_str, Public)
FIELD_Index(btree_each_column_u32_u64_str, id)
FIELD_Index(btree_each_column_u32_u64_str, age)
FIELD_Index(btree_each_column_u32_u64_str, name)

// =============================================================================
// SYNTHETIC BENCHMARK - TABLE VARIANTS FOR u32_u64_u64 (id, x, y)
// =============================================================================

// Unique indexed version
struct unique_0_u32_u64_u64_t {
    uint32_t id;
    uint64_t x;
    uint64_t y;
};
SPACETIMEDB_STRUCT(unique_0_u32_u64_u64_t, id, x, y)
SPACETIMEDB_TABLE(unique_0_u32_u64_u64_t, unique_0_u32_u64_u64, Public)
FIELD_Unique(unique_0_u32_u64_u64, id)

// No index version
struct no_index_u32_u64_u64_t {
    uint32_t id;
    uint64_t x;
    uint64_t y;
};
SPACETIMEDB_STRUCT(no_index_u32_u64_u64_t, id, x, y)
SPACETIMEDB_TABLE(no_index_u32_u64_u64_t, no_index_u32_u64_u64, Public)

// BTree index on each column version
struct btree_each_column_u32_u64_u64_t {
    uint32_t id;
    uint64_t x;
    uint64_t y;
};
SPACETIMEDB_STRUCT(btree_each_column_u32_u64_u64_t, id, x, y)
SPACETIMEDB_TABLE(btree_each_column_u32_u64_u64_t, btree_each_column_u32_u64_u64, Public)
FIELD_Index(btree_each_column_u32_u64_u64, id)
FIELD_Index(btree_each_column_u32_u64_u64, x)
FIELD_Index(btree_each_column_u32_u64_u64, y)

// =============================================================================
// SYNTHETIC BENCHMARK - EMPTY REDUCER FOR BASELINE
// =============================================================================

// Empty reducer for baseline measurement
SPACETIMEDB_REDUCER(empty, ReducerContext& ctx) {
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - SINGLE INSERT OPERATIONS FOR STRING TABLES
// =============================================================================

SPACETIMEDB_REDUCER(insert_unique_0_u32_u64_str, ReducerContext& ctx, uint32_t id, uint64_t age, std::string name) {
    unique_0_u32_u64_str_t record = {id, age, name};
    ctx.db[unique_0_u32_u64_str].insert(record);
    return Ok();
}

SPACETIMEDB_REDUCER(insert_no_index_u32_u64_str, ReducerContext& ctx, uint32_t id, uint64_t age, std::string name) {
    no_index_u32_u64_str_t record = {id, age, name};
    ctx.db[no_index_u32_u64_str].insert(record);
    return Ok();
}

SPACETIMEDB_REDUCER(insert_btree_each_column_u32_u64_str, ReducerContext& ctx, uint32_t id, uint64_t age, std::string name) {
    btree_each_column_u32_u64_str_t record = {id, age, name};
    ctx.db[btree_each_column_u32_u64_str].insert(record);
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - SINGLE INSERT OPERATIONS FOR NUMERIC TABLES
// =============================================================================

SPACETIMEDB_REDUCER(insert_unique_0_u32_u64_u64, ReducerContext& ctx, uint32_t id, uint64_t x, uint64_t y) {
    unique_0_u32_u64_u64_t record = {id, x, y};
    ctx.db[unique_0_u32_u64_u64].insert(record);
    return Ok();
}

SPACETIMEDB_REDUCER(insert_no_index_u32_u64_u64, ReducerContext& ctx, uint32_t id, uint64_t x, uint64_t y) {
    no_index_u32_u64_u64_t record = {id, x, y};
    ctx.db[no_index_u32_u64_u64].insert(record);
    return Ok();
}

SPACETIMEDB_REDUCER(insert_btree_each_column_u32_u64_u64, ReducerContext& ctx, uint32_t id, uint64_t x, uint64_t y) {
    btree_each_column_u32_u64_u64_t record = {id, x, y};
    ctx.db[btree_each_column_u32_u64_u64].insert(record);
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - BULK INSERT OPERATIONS FOR NUMERIC TABLES
// =============================================================================

SPACETIMEDB_REDUCER(insert_bulk_unique_0_u32_u64_u64, ReducerContext& ctx, std::vector<unique_0_u32_u64_u64_t> locs) {
    for (const auto& loc : locs) {
        ctx.db[unique_0_u32_u64_u64].insert(loc);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(insert_bulk_no_index_u32_u64_u64, ReducerContext& ctx, std::vector<no_index_u32_u64_u64_t> locs) {
    for (const auto& loc : locs) {
        ctx.db[no_index_u32_u64_u64].insert(loc);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(insert_bulk_btree_each_column_u32_u64_u64, ReducerContext& ctx, std::vector<btree_each_column_u32_u64_u64_t> locs) {
    for (const auto& loc : locs) {
        ctx.db[btree_each_column_u32_u64_u64].insert(loc);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - BULK INSERT OPERATIONS FOR STRING TABLES
// =============================================================================

SPACETIMEDB_REDUCER(insert_bulk_unique_0_u32_u64_str, ReducerContext& ctx, std::vector<unique_0_u32_u64_str_t> people) {
    for (const auto& person : people) {
        ctx.db[unique_0_u32_u64_str].insert(person);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(insert_bulk_no_index_u32_u64_str, ReducerContext& ctx, std::vector<no_index_u32_u64_str_t> people) {
    for (const auto& person : people) {
        ctx.db[no_index_u32_u64_str].insert(person);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(insert_bulk_btree_each_column_u32_u64_str, ReducerContext& ctx, std::vector<btree_each_column_u32_u64_str_t> people) {
    for (const auto& person : people) {
        ctx.db[btree_each_column_u32_u64_str].insert(person);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - UPDATE OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(update_bulk_unique_0_u32_u64_u64, ReducerContext& ctx, uint32_t row_count) {
    uint32_t hit = 0;
    for (const auto& u32_u64_u64 : ctx.db[unique_0_u32_u64_u64]) {
        if (hit >= row_count) break;
        ++hit;
        unique_0_u32_u64_u64_t updated_loc = {u32_u64_u64.id, u32_u64_u64.x + 1, u32_u64_u64.y};
        ctx.db[unique_0_u32_u64_u64_id].update(updated_loc);
    }
    if (hit != row_count) {
        return Err("Not enough rows to perform requested amount of updates");
    }
    return Ok();
}

SPACETIMEDB_REDUCER(update_bulk_unique_0_u32_u64_str, ReducerContext& ctx, uint32_t row_count) {
    uint32_t hit = 0;
    for (const auto& u32_u64_str : ctx.db[unique_0_u32_u64_str]) {
        if (hit >= row_count) break;
        ++hit;
        unique_0_u32_u64_str_t updated = {u32_u64_str.id, u32_u64_str.age + 1, u32_u64_str.name};
        ctx.db[unique_0_u32_u64_str_id].update(updated);
    }
    if (hit != row_count) {
        return Err("Not enough rows to perform requested amount of updates");
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - ITERATION OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(iterate_unique_0_u32_u64_str, ReducerContext& ctx) {
    for (const auto& u32_u64_str : ctx.db[unique_0_u32_u64_str]) {
        black_box(u32_u64_str);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(iterate_unique_0_u32_u64_u64, ReducerContext& ctx) {
    for (const auto& u32_u64_u64 : ctx.db[unique_0_u32_u64_u64]) {
        black_box(u32_u64_u64);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - FILTERING BY ID OPERATIONS FOR STRING TABLES
// =============================================================================

SPACETIMEDB_REDUCER(filter_unique_0_u32_u64_str_by_id, ReducerContext& ctx, uint32_t id) {
    auto result = ctx.db[unique_0_u32_u64_str_id].find(id);
    if (result) {
        black_box(*result);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_no_index_u32_u64_str_by_id, ReducerContext& ctx, uint32_t id) {
    for (const auto& r : ctx.db[no_index_u32_u64_str]) {
        if (r.id == id) {
            black_box(r);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_btree_each_column_u32_u64_str_by_id, ReducerContext& ctx, uint32_t id) {
    for (const auto& r : ctx.db[btree_each_column_u32_u64_str_id].filter(id)) {
        black_box(r);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - FILTERING BY NAME OPERATIONS FOR STRING TABLES
// =============================================================================

SPACETIMEDB_REDUCER(filter_unique_0_u32_u64_str_by_name, ReducerContext& ctx, std::string name) {
    for (const auto& p : ctx.db[unique_0_u32_u64_str]) {
        if (p.name == name) {
            black_box(p);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_no_index_u32_u64_str_by_name, ReducerContext& ctx, std::string name) {
    for (const auto& p : ctx.db[no_index_u32_u64_str]) {
        if (p.name == name) {
            black_box(p);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_btree_each_column_u32_u64_str_by_name, ReducerContext& ctx, std::string name) {
    for (const auto& p : ctx.db[btree_each_column_u32_u64_str_name].filter(name)) {
        black_box(p);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - FILTERING BY ID OPERATIONS FOR NUMERIC TABLES
// =============================================================================

SPACETIMEDB_REDUCER(filter_unique_0_u32_u64_u64_by_id, ReducerContext& ctx, uint32_t id) {
    auto result = ctx.db[unique_0_u32_u64_u64_id].find(id);
    if (result) {
        black_box(*result);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_no_index_u32_u64_u64_by_id, ReducerContext& ctx, uint32_t id) {
    for (const auto& loc : ctx.db[no_index_u32_u64_u64]) {
        if (loc.id == id) {
            black_box(loc);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_btree_each_column_u32_u64_u64_by_id, ReducerContext& ctx, uint32_t id) {
    for (const auto& loc : ctx.db[btree_each_column_u32_u64_u64_id].filter(id)) {
        black_box(loc);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - FILTERING BY X COORDINATE FOR NUMERIC TABLES
// =============================================================================

SPACETIMEDB_REDUCER(filter_unique_0_u32_u64_u64_by_x, ReducerContext& ctx, uint64_t x) {
    for (const auto& loc : ctx.db[unique_0_u32_u64_u64]) {
        if (loc.x == x) {
            black_box(loc);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_no_index_u32_u64_u64_by_x, ReducerContext& ctx, uint64_t x) {
    for (const auto& loc : ctx.db[no_index_u32_u64_u64]) {
        if (loc.x == x) {
            black_box(loc);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_btree_each_column_u32_u64_u64_by_x, ReducerContext& ctx, uint64_t x) {
    for (const auto& loc : ctx.db[btree_each_column_u32_u64_u64_x].filter(x)) {
        black_box(loc);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - FILTERING BY Y COORDINATE FOR NUMERIC TABLES
// =============================================================================

SPACETIMEDB_REDUCER(filter_unique_0_u32_u64_u64_by_y, ReducerContext& ctx, uint64_t y) {
    for (const auto& loc : ctx.db[unique_0_u32_u64_u64]) {
        if (loc.y == y) {
            black_box(loc);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_no_index_u32_u64_u64_by_y, ReducerContext& ctx, uint64_t y) {
    for (const auto& loc : ctx.db[no_index_u32_u64_u64]) {
        if (loc.y == y) {
            black_box(loc);
        }
    }
    return Ok();
}

SPACETIMEDB_REDUCER(filter_btree_each_column_u32_u64_u64_by_y, ReducerContext& ctx, uint64_t y) {
    for (const auto& loc : ctx.db[btree_each_column_u32_u64_u64_y].filter(y)) {
        black_box(loc);
    }
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - DELETE OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(delete_unique_0_u32_u64_str_by_id, ReducerContext& ctx, uint32_t id) {
    ctx.db[unique_0_u32_u64_str_id].delete_by_value(id);
    return Ok();
}

SPACETIMEDB_REDUCER(delete_unique_0_u32_u64_u64_by_id, ReducerContext& ctx, uint32_t id) {
    ctx.db[unique_0_u32_u64_u64_id].delete_by_value(id);
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - CLEAR TABLE OPERATIONS (UNIMPLEMENTED)
// =============================================================================

SPACETIMEDB_REDUCER(clear_table_unique_0_u32_u64_str, ReducerContext& ctx) {
    return Err("Modules currently have no interface to clear a table");
    
}

SPACETIMEDB_REDUCER(clear_table_no_index_u32_u64_str, ReducerContext& ctx) {
    return Err("Modules currently have no interface to clear a table");
}

SPACETIMEDB_REDUCER(clear_table_btree_each_column_u32_u64_str, ReducerContext& ctx) {
    return Err("Modules currently have no interface to clear a table");
}

SPACETIMEDB_REDUCER(clear_table_unique_0_u32_u64_u64, ReducerContext& ctx) {
    return Err("Modules currently have no interface to clear a table");
}

SPACETIMEDB_REDUCER(clear_table_no_index_u32_u64_u64, ReducerContext& ctx) {
    return Err("Modules currently have no interface to clear a table");
}

SPACETIMEDB_REDUCER(clear_table_btree_each_column_u32_u64_u64, ReducerContext& ctx) {
    return Err("Modules currently have no interface to clear a table");
}

// =============================================================================
// SYNTHETIC BENCHMARK - COUNT OPERATIONS
// =============================================================================

SPACETIMEDB_REDUCER(count_unique_0_u32_u64_str, ReducerContext& ctx) {
    LOG_INFO("COUNT: " + std::to_string(ctx.db[unique_0_u32_u64_str].count()));
    return Ok();
}

SPACETIMEDB_REDUCER(count_no_index_u32_u64_str, ReducerContext& ctx) {
    LOG_INFO("COUNT: " + std::to_string(ctx.db[no_index_u32_u64_str].count()));
    return Ok();
}

SPACETIMEDB_REDUCER(count_btree_each_column_u32_u64_str, ReducerContext& ctx) {
    LOG_INFO("COUNT: " + std::to_string(ctx.db[btree_each_column_u32_u64_str].count()));
    return Ok();
}

SPACETIMEDB_REDUCER(count_unique_0_u32_u64_u64, ReducerContext& ctx) {
    LOG_INFO("COUNT: " + std::to_string(ctx.db[unique_0_u32_u64_u64].count()));
    return Ok();
}

SPACETIMEDB_REDUCER(count_no_index_u32_u64_u64, ReducerContext& ctx) {
    LOG_INFO("COUNT: " + std::to_string(ctx.db[no_index_u32_u64_u64].count()));
    return Ok();
}

SPACETIMEDB_REDUCER(count_btree_each_column_u32_u64_u64, ReducerContext& ctx) {
    LOG_INFO("COUNT: " + std::to_string(ctx.db[btree_each_column_u32_u64_u64].count()));
    return Ok();
}

// =============================================================================
// SYNTHETIC BENCHMARK - MODULE-SPECIFIC STRESS TESTING
// =============================================================================

SPACETIMEDB_REDUCER(fn_with_1_args, ReducerContext& ctx, std::string arg) {
    return Ok();    
}

SPACETIMEDB_REDUCER(fn_with_32_args, ReducerContext& ctx,
    std::string arg1, std::string arg2, std::string arg3, std::string arg4,
    std::string arg5, std::string arg6, std::string arg7, std::string arg8,
    std::string arg9, std::string arg10, std::string arg11, std::string arg12,
    std::string arg13, std::string arg14, std::string arg15, std::string arg16,
    std::string arg17, std::string arg18, std::string arg19, std::string arg20,
    std::string arg21, std::string arg22, std::string arg23, std::string arg24,
    std::string arg25, std::string arg26, std::string arg27, std::string arg28,
    std::string arg29, std::string arg30, std::string arg31, std::string arg32) {
    return Ok();
}

SPACETIMEDB_REDUCER(print_many_things, ReducerContext& ctx, uint32_t n) {
    for (uint32_t i = 0; i < n; ++i) {
        LOG_INFO("hello again!");
    }
    return Ok();
}