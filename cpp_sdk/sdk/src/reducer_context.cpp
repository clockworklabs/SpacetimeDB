#include <spacetimedb/sdk/reducer_context.h>
#include <spacetimedb/sdk/database.h> // Required for the Database& member

namespace spacetimedb {
namespace sdk {

ReducerContext::ReducerContext(Identity sender, Timestamp timestamp, Database& db_instance)
    : current_sender(std::move(sender)),
      current_timestamp(timestamp),
      database_instance(db_instance) {}

const Identity& ReducerContext::get_sender() const {
    return current_sender;
}

Timestamp ReducerContext::get_timestamp() const {
    return current_timestamp;
}

Database& ReducerContext::db() {
    return database_instance;
}

const Database& ReducerContext::db() const {
    return database_instance;
}

} // namespace sdk
} // namespace spacetimedb
