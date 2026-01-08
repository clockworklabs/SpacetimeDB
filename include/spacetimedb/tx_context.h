#ifndef SPACETIMEDB_TX_CONTEXT_H
#define SPACETIMEDB_TX_CONTEXT_H

#include <spacetimedb/database.h>
#include <spacetimedb/bsatn/timestamp.h>

namespace SpacetimeDb {

/**
 * @brief Transaction context for procedures
 * 
 * TxContext provides database access within a procedure transaction.
 * It's analogous to Rust's TxContext which is passed to closures in
 * `ctx.with_tx()` and `ctx.try_with_tx()`.
 * 
 * Key characteristics:
 * - Provides read-write database access via `db` field
 * - All database operations are part of an anonymous transaction
 * - Transaction commits when the callback returns successfully
 * - Transaction rolls back if the callback throws or returns error
 * 
 * Example usage:
 * @code
 * SPACETIMEDB_PROCEDURE(void, insert_user, ProcedureContext ctx, std::string name) {
 *     ctx.with_tx([&](TxContext& tx) {
 *         // Database operations here are transactional
 *         tx.db.users().insert(User{name});
 *     });
 * }
 * @endcode
 */
struct TxContext {
    // Database access - name-based like ReducerContext
    DatabaseContext db;
    
    // Timestamp of the transaction
    // Note: In procedures, this may be updated if transaction is retried
    Timestamp timestamp;
    
    // Constructor
    TxContext(Timestamp ts) : timestamp(ts) {}
};

} // namespace SpacetimeDb

#endif // SPACETIMEDB_TX_CONTEXT_H
