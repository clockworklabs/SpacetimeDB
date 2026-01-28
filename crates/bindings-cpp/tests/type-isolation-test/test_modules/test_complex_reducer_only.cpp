#include <spacetimedb.h>
#include <optional>

using namespace SpacetimeDB;


// Complex struct with special types
struct EveryPrimitiveStruct {
    uint8_t a;
    uint16_t b;
    uint32_t c;
    uint64_t d;
    u128 e;
    u256 f;
    int8_t g;
    int16_t h;
    int32_t i;
    int64_t j;
    i128 k;
    i256 l;
    bool m;
    float n;
    double o;
    std::string p;
    Identity q;
    ConnectionId r;
    Timestamp s;
    TimeDuration t;
};
SPACETIMEDB_STRUCT(EveryPrimitiveStruct, a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p, q, r, s, t)

// Simple table without the complex struct
struct SimpleTable {
    int32_t id;
    std::string value;
};
SPACETIMEDB_STRUCT(SimpleTable, id, value)
SPACETIMEDB_TABLE(SimpleTable, simple_table, Public)

// Reducer that uses the complex struct as parameter
SPACETIMEDB_REDUCER(insert_with_complex, ReducerContext ctx, EveryPrimitiveStruct data)
{
    SimpleTable row;
    row.id = data.i;  // Use the int32_t field as id
    row.value = data.p;  // Use the string field as value
    ctx.db.table<SimpleTable>("simple_table").insert(row);
}

// Another reducer without complex struct
SPACETIMEDB_REDUCER(insert_simple, ReducerContext ctx, int32_t id, std::string value)
{
    SimpleTable row;
    row.id = id;
    row.value = value;
    ctx.db.table<SimpleTable>("simple_table").insert(row);
}