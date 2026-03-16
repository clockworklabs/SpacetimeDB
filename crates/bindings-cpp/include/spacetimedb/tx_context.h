#ifndef SPACETIMEDB_TX_CONTEXT_H
#define SPACETIMEDB_TX_CONTEXT_H

#include <spacetimedb/reducer_context.h>

namespace SpacetimeDB {

/**
 * @brief Transaction context for procedures
 * 
 * TxContext wraps a ReducerContext to provide transactional database access.
 * It's analogous to Rust's TxContext which is passed to closures in
 * `ctx.with_tx()` and `ctx.try_with_tx()`.
 * 
 * Design: Mimics Rust's Deref trait for consistent API
 * =====================================================
 * In Rust, TxContext implements Deref<Target=ReducerContext>, which means:
 *   - Reducers use: ctx.db.table()
 *   - Transactions use: tx.db.table()  (Deref auto-dereferences)
 * 
 * C++ doesn't support operator. overloading, so we explicitly expose
 * ReducerContext fields as public references to achieve the same ergonomics:
 *   - Reducers use: ctx.db[table]
 *   - Transactions use: tx.db[table]  (same syntax!)
 * 
 * Tradeoff: TxContext is 40 bytes instead of 8 bytes (storing 5 references),
 * but this is negligible as TxContext is stack-allocated and short-lived.
 * The consistent API is worth the minor memory cost.
 * 
 * All database operations are part of an anonymous transaction:
 * - Transaction commits when the callback returns successfully
 * - Transaction rolls back if the callback throws or returns error
 * 
 * Example usage:
 * @code
 * SPACETIMEDB_PROCEDURE(void, insert_user, ProcedureContext ctx, std::string name) {
 *     ctx.with_tx([&](TxContext& tx) {
 *         // Access authentication (same as in reducers)
 *         if (tx.sender_auth().has_jwt()) {
 *             auto jwt = tx.sender_auth().get_jwt();
 *             // ...
 *         }
 *         // Database operations here are transactional (same syntax as reducers)
 *         tx.db[users].insert(User{name});
 *     });
 * }
 * @endcode
 */
struct TxContext {
private:
    ReducerContext& ctx_;
    
public:
    // Public references to ReducerContext fields for consistent API with Rust
    // In Rust, Deref makes tx.db work the same as ctx.db
    // In C++, we explicitly expose references to achieve the same ergonomics
    DatabaseContext& db;
    const Identity& sender;
    const Timestamp& timestamp;
    const std::optional<ConnectionId>& connection_id;
    
    // Constructor - initializes all reference members
    explicit TxContext(ReducerContext& ctx) 
        : ctx_(ctx), 
          db(ctx.db),
          sender(ctx.sender),
          timestamp(ctx.timestamp),
          connection_id(ctx.connection_id) {}
    
    // Access to ReducerContext methods
    const AuthCtx& sender_auth() const { return ctx_.sender_auth(); }
    Identity identity() const { return ctx_.identity(); }
    StdbRng& rng() const { return ctx_.rng(); }
    
    /**
     * Generate a new random UUID v4.
     * 
     * Creates a random UUID using the transaction's deterministic RNG.
     * 
     * Example:
     * @code
     * SPACETIMEDB_PROCEDURE(void, create_session, ProcedureContext ctx) {
     *     ctx.with_tx([&](TxContext& tx) {
     *         Uuid session_id = tx.new_uuid_v4();
     *         tx.db[sessions].insert(Session{session_id});
     *     });
     * }
     * @endcode
     * 
     * @return A new UUID v4
     */
    Uuid new_uuid_v4() const { return ctx_.new_uuid_v4(); }
    
    /**
     * Generate a new UUID v7.
     * 
     * Creates a time-ordered UUID with the transaction's timestamp, a monotonic counter,
     * and random bytes from the transaction's deterministic RNG.
     * 
     * Example:
     * @code
     * SPACETIMEDB_PROCEDURE(void, create_user, ProcedureContext ctx, std::string name) {
     *     ctx.with_tx([&](TxContext& tx) {
     *         Uuid user_id = tx.new_uuid_v7();
     *         tx.db[users].insert(User{user_id, name});
     *     });
     * }
     * @endcode
     * 
     * @return A new UUID v7
     */
    Uuid new_uuid_v7() const { return ctx_.new_uuid_v7(); }
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_TX_CONTEXT_H
