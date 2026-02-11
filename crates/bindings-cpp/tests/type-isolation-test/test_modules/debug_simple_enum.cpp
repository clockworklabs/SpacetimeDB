#include <spacetimedb.h>

using namespace SpacetimeDB;


// Simple enum using new unified syntax
SPACETIMEDB_ENUM(SimpleEnum, Zero, One, Two)

// Complex enum using new unified syntax
SPACETIMEDB_ENUM(TestEnum,
    (SimpleEnums, std::vector<SimpleEnum>)
)

// Table that uses SimpleEnum directly
struct SimpleEnumTable { 
    SimpleEnum e; 
    int32_t id;
};
SPACETIMEDB_STRUCT(SimpleEnumTable, e, id)
SPACETIMEDB_TABLE(SimpleEnumTable, simple_enum_table, Public)

// Table that uses TestEnum (containing vector<SimpleEnum>)
struct TestEnumTable { 
    TestEnum te;
    int32_t id;
};
SPACETIMEDB_STRUCT(TestEnumTable, te, id)
SPACETIMEDB_TABLE(TestEnumTable, test_enum_table, Public)

// Reducer that uses SimpleEnum as parameter - this might be the trigger
SPACETIMEDB_REDUCER(insert_enum, ReducerContext ctx, SimpleEnum e, int32_t id)
{
    ctx.db.table<SimpleEnumTable>("simple_enum_table").insert(SimpleEnumTable{e, id});
}