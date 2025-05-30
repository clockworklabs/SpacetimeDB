#include "reducer_context.h"
#include "database.h" // Now includes the actual Database header.

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
