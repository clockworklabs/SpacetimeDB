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

// Table with the complex struct as a field
struct ComplexTable {
    int32_t id;
    EveryPrimitiveStruct data;
};
SPACETIMEDB_STRUCT(ComplexTable, id, data)
SPACETIMEDB_TABLE(ComplexTable, complex_table, Public)

// Reducer that doesn't use the complex struct as parameter
SPACETIMEDB_REDUCER(insert_default, ReducerContext ctx, int32_t id)
{
    ComplexTable row;
    row.id = id;
    // Initialize struct fields explicitly
    row.data.a = 1;
    row.data.b = 2;
    row.data.c = 3;
    row.data.d = 4;
    row.data.e = u128{5};
    row.data.f = u256();  // Default constructor
    row.data.g = 7;
    row.data.h = 8;
    row.data.i = 9;
    row.data.j = 10;
    row.data.k = i128{11};
    row.data.l = i256();  // Default constructor
    row.data.m = true;
    row.data.n = 14.0f;
    row.data.o = 15.0;
    row.data.p = "test";
    row.data.q = Identity();  // Default constructor
    row.data.r = ConnectionId{u128{17}};
    row.data.s = Timestamp::now();
    row.data.t = TimeDuration{100};
    ctx.db.table<ComplexTable>("complex_table").insert(row);
}