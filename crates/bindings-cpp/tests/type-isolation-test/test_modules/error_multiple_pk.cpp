#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test multiple primary keys in a single table
// SpacetimeDB should only allow ONE primary key per table

// Table with two PrimaryKey fields - INVALID
struct DoublePrimaryKey {
    uint32_t id1;
    uint32_t id2;
    std::string data;
};
SPACETIMEDB_STRUCT(DoublePrimaryKey, id1, id2, data)
SPACETIMEDB_TABLE(DoublePrimaryKey, double_pk_table, SpacetimeDB::Public)
FIELD_PrimaryKey(double_pk_table, id1);
FIELD_PrimaryKey(double_pk_table, id2);  // ERROR: Two primary keys!

// Table with PrimaryKey and PrimaryKeyAutoInc - INVALID
struct MixedPrimaryKey {
    uint32_t manual_id;
    uint64_t auto_id;
    std::string data;
};
SPACETIMEDB_STRUCT(MixedPrimaryKey, manual_id, auto_id, data)
SPACETIMEDB_TABLE(MixedPrimaryKey, mixed_pk_table, SpacetimeDB::Public)
FIELD_PrimaryKey(mixed_pk_table, manual_id);
FIELD_PrimaryKeyAutoInc(mixed_pk_table, auto_id);  // ERROR: Two primary keys of different types!

// Table with multiple PrimaryKeyAutoInc - INVALID
struct DoubleAutoInc {
    uint64_t id1;
    uint64_t id2;
    std::string data;
};
SPACETIMEDB_STRUCT(DoubleAutoInc, id1, id2, data)
SPACETIMEDB_TABLE(DoubleAutoInc, double_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(double_autoinc_table, id1);
FIELD_PrimaryKeyAutoInc(double_autoinc_table, id2);  // ERROR: Two auto-increment primary keys!

// Table with three primary keys - VERY INVALID
struct TriplePrimaryKey {
    uint32_t id1;
    uint32_t id2;
    uint32_t id3;
    std::string data;
};
SPACETIMEDB_STRUCT(TriplePrimaryKey, id1, id2, id3, data)
SPACETIMEDB_TABLE(TriplePrimaryKey, triple_pk_table, SpacetimeDB::Public)
FIELD_PrimaryKey(triple_pk_table, id1);
FIELD_PrimaryKey(triple_pk_table, id2);
FIELD_PrimaryKey(triple_pk_table, id3);  // ERROR: Three primary keys!

// Valid table for comparison - single primary key
struct SinglePrimaryKey {
    uint32_t id;
    std::string data;
};
SPACETIMEDB_STRUCT(SinglePrimaryKey, id, data)
SPACETIMEDB_TABLE(SinglePrimaryKey, single_pk_table, SpacetimeDB::Public)
FIELD_PrimaryKey(single_pk_table, id);  // Correct: Single primary key

// Valid table with auto-increment
struct SingleAutoInc {
    uint64_t id;
    std::string data;
};
SPACETIMEDB_STRUCT(SingleAutoInc, id, data)
SPACETIMEDB_TABLE(SingleAutoInc, single_autoinc_table, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(single_autoinc_table, id);  // Correct: Single auto-increment primary key

// Test reducer
SPACETIMEDB_REDUCER(test_multiple_pks, SpacetimeDB::ReducerContext ctx)
{
    LOG_INFO("Testing multiple primary keys - should fail validation");
    
    // Try to insert into double PK table
    DoublePrimaryKey double_pk{1, 2, "Double PK"};
    ctx.db[double_pk_table].insert(double_pk);
    
    // Try to insert into mixed PK table
    MixedPrimaryKey mixed_pk{1, 0, "Mixed PK"};
    ctx.db[mixed_pk_table].insert(mixed_pk);
    
    // Insert into valid tables
    SinglePrimaryKey single_pk{1, "Valid single PK"};
    ctx.db[single_pk_table].insert(single_pk);
    
    SingleAutoInc single_auto{0, "Valid auto-inc"};
    ctx.db[single_autoinc_table].insert(single_auto);
    return Ok();
}

// Init reducer
SPACETIMEDB_INIT(init, ReducerContext ctx)
{
    LOG_INFO("Multiple primary keys test - should fail validation");
    LOG_INFO("SpacetimeDB allows only ONE primary key per table");
    return Ok();
}