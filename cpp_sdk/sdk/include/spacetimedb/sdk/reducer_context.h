#ifndef REDUCER_CONTEXT_H
#define REDUCER_CONTEXT_H

#include <spacetimedb/sdk/spacetimedb_sdk_types.h> // For Identity, Timestamp

namespace spacetimedb {
namespace sdk {

// Forward declaration
class Database;

class ReducerContext {
public:
    // Constructor: Typically called by the SDK internals.
    // It needs access to the current transaction's sender identity, timestamp,
    // and a way to interact with the database.
    ReducerContext(Identity sender, Timestamp timestamp, Database& db_instance);

    // Gets the identity of the client/principal that initiated the transaction.
    const Identity& get_sender() const;

    // Gets the timestamp of the current transaction.
    Timestamp get_timestamp() const;

    // Provides access to database operations.
    Database& db();
    const Database& db() const; // Const overload


private:
    Identity current_sender;
    Timestamp current_timestamp;
    Database& database_instance;
    // Note: Storing a reference to Database implies Database lifetime management
    // is handled externally and outlives ReducerContext.
};

} // namespace sdk
} // namespace spacetimedb

#endif // REDUCER_CONTEXT_H
