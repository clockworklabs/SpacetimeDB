#include <spacetimedb.h>

using namespace SpacetimeDB;

// Test scheduled table without primary key on scheduled_id
// This should be caught as an error

// Incorrect scheduled table - scheduled_id is NOT a primary key
struct BadScheduledTable {
    uint64_t scheduled_id;  // Should be PrimaryKeyAutoInc but isn't
    SpacetimeDB::ScheduleAt scheduled_at;
    std::string message;
};
SPACETIMEDB_STRUCT(BadScheduledTable, scheduled_id, scheduled_at, message)
SPACETIMEDB_TABLE(BadScheduledTable, bad_scheduled, SpacetimeDB::Public)  // Missing PrimaryKeyAutoInc(scheduled_id)
SPACETIMEDB_SCHEDULE(bad_scheduled, 1, process_bad_schedule)  // This should fail!

// Another incorrect variant - has primary key but on wrong field
struct WrongPkScheduledTable {
    uint64_t scheduled_id;
    SpacetimeDB::ScheduleAt scheduled_at;
    std::string message;
};
SPACETIMEDB_STRUCT(WrongPkScheduledTable, scheduled_id, scheduled_at, message)
SPACETIMEDB_TABLE(WrongPkScheduledTable, wrong_pk_scheduled, SpacetimeDB::Public)
FIELD_PrimaryKey(wrong_pk_scheduled, message)  // Primary key on wrong field!
SPACETIMEDB_SCHEDULE(wrong_pk_scheduled, 1, process_wrong_pk_schedule)

// Yet another incorrect variant - has unique constraint instead of primary key
struct UniqueScheduledTable {
    uint64_t scheduled_id;
    SpacetimeDB::ScheduleAt scheduled_at;
    std::string message;
};
SPACETIMEDB_STRUCT(UniqueScheduledTable, scheduled_id, scheduled_at, message)
SPACETIMEDB_TABLE(UniqueScheduledTable, unique_scheduled, SpacetimeDB::Public)
FIELD_Unique(unique_scheduled, scheduled_id)  // Unique instead of PrimaryKey!
SPACETIMEDB_SCHEDULE(unique_scheduled, 1, process_unique_schedule)

// Correct scheduled table for comparison
struct GoodScheduledTable {
    uint64_t scheduled_id;
    SpacetimeDB::ScheduleAt scheduled_at;
    std::string message;
};
SPACETIMEDB_STRUCT(GoodScheduledTable, scheduled_id, scheduled_at, message)
SPACETIMEDB_TABLE(GoodScheduledTable, good_scheduled, SpacetimeDB::Public)
FIELD_PrimaryKeyAutoInc(good_scheduled, scheduled_id)  // Correct!
SPACETIMEDB_SCHEDULE(good_scheduled, 1, process_good_schedule)

// Scheduled reducers
SPACETIMEDB_REDUCER(process_bad_schedule, SpacetimeDB::ReducerContext ctx, BadScheduledTable arg)
{
    LOG_INFO("Bad schedule executed: " + arg.message);
    return Ok();
}

SPACETIMEDB_REDUCER(process_wrong_pk_schedule, SpacetimeDB::ReducerContext ctx, WrongPkScheduledTable arg)
{
    LOG_INFO("Wrong PK schedule executed: " + arg.message);
    return Ok();
}

SPACETIMEDB_REDUCER(process_unique_schedule, SpacetimeDB::ReducerContext ctx, UniqueScheduledTable arg)
{
    LOG_INFO("Unique schedule executed: " + arg.message);
    return Ok();
}

SPACETIMEDB_REDUCER(process_good_schedule, SpacetimeDB::ReducerContext ctx, GoodScheduledTable arg)
{
    LOG_INFO("Good schedule executed: " + arg.message);
    return Ok();
}

// Test reducer to schedule tasks
SPACETIMEDB_REDUCER(test_schedule_tables, SpacetimeDB::ReducerContext ctx)
{
    LOG_INFO("Testing scheduled tables - this should fail if validation works");
    
    // Try to insert into bad scheduled table
    BadScheduledTable bad{0, ScheduleAt::time(ctx.timestamp + TimeDuration(1000000)), "Bad schedule"};
    ctx.db[bad_scheduled].insert(bad);
    
    // Try to insert into wrong PK scheduled table
    WrongPkScheduledTable wrong{0, ScheduleAt::time(ctx.timestamp + TimeDuration(1000000)), "Wrong PK"};
    ctx.db[wrong_pk_scheduled].insert(wrong);
    
    // Try to insert into unique scheduled table
    UniqueScheduledTable unique{0, ScheduleAt::time(ctx.timestamp + TimeDuration(1000000)), "Unique instead of PK"};
    ctx.db[unique_scheduled].insert(unique);
    
    // Insert into good scheduled table (should work)
    GoodScheduledTable good{0, ScheduleAt::time(ctx.timestamp + TimeDuration(1000000)), "Good schedule"};
    ctx.db[good_scheduled].insert(good);
    return Ok();
}

// Init reducer
SPACETIMEDB_INIT(init, ReducerContext ctx)
{
    LOG_INFO("Scheduled table PK test module initialized");
    LOG_INFO("This tests that scheduled_id must be a primary key");
    LOG_INFO("Should fail if scheduled_id is not PrimaryKeyAutoInc");
    return Ok();
}