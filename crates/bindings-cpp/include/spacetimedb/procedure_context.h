#ifndef SPACETIMEDB_PROCEDURE_CONTEXT_H
#define SPACETIMEDB_PROCEDURE_CONTEXT_H

#include <spacetimedb/bsatn/types.h> // For Identity
#include <spacetimedb/bsatn/timestamp.h> // For Timestamp
#include <spacetimedb/tx_context.h> // For TxContext
#include <spacetimedb/abi/FFI.h> // For transaction syscalls
#include <cstdint>
#include <functional>
#include <stdexcept>
#include <type_traits>

namespace SpacetimeDb {

/**
 * @brief Context for procedures
 * 
 * ProcedureContext provides access to call metadata (sender, timestamp, connection)
 * but does NOT have direct database access. This is a key difference from ReducerContext.
 * 
 * Part 1 Implementation: Pure Functions
 * - Procedures can perform computations and return results
 * - NO database access (no db field)
 * - Stateless operations only
 * 
 * Future Parts (documented for reference):
 * - Part 2: Transactions via ctx.WithTx() and ctx.TryWithTx()
 * - Part 3: Scheduled execution via table attributes
 * - Part 4: HTTP requests via HttpClient
 * 
 * Key differences from ReducerContext:
 * - NO db field (database operations require explicit transactions in Part 2)
 * - Has connection_id (procedures track which connection called them)
 * - NO rng() method (procedures should be deterministic)
 * 
 * Example usage (Part 1 - pure function):
 * @code
 * SPACETIMEDB_PROCEDURE(uint32_t, add_numbers, ProcedureContext ctx, uint32_t a, uint32_t b) {
 *     // Part 1: Can only do computation, no database access
 *     if (a == 0 && b == 0) {
 *         return Err("Cannot add two zeros");
 *     }
 *     return Ok(a + b);
 * }
 * @endcode
 * 
 * Future example (Part 2 - with transactions):
 * @code
 * SPACETIMEDB_PROCEDURE(Unit, insert_item, ProcedureContext ctx, Item item) {
 *     // Part 2: Database operations require explicit transaction
 *     ctx.WithTx([&item](TxContext& tx) {
 *         tx.db[items].insert(item);
 *     });
 *     return Ok();
 * }
 * @endcode
 */
struct ProcedureContext {
    // Caller's identity - who invoked this procedure
    Identity sender;
    
    // Timestamp when the procedure was invoked
    Timestamp timestamp;
    
    // Connection ID for the caller
    // Used to track which client connection initiated this procedure
    ConnectionId connection_id;
    
    // NOTE: NO db field!
    // Part 1 procedures are pure functions - no database access
    // Part 2 will add WithTx() and TryWithTx() methods for transactions
    
    // Constructors
    ProcedureContext() = default;
    
    ProcedureContext(Identity s, Timestamp t, ConnectionId conn_id)
        : sender(s), timestamp(t), connection_id(conn_id) {}

#ifdef SPACETIMEDB_UNSTABLE_FEATURES
    /**
     * @brief Execute a callback within a database transaction
     * 
     * Starts a mutable transaction, executes the callback, and commits on success.
     * If the callback panics (via LOG_PANIC), the transaction is automatically rolled back.
     * 
     * The callback receives a TxContext with database access. All database operations
     * performed within the callback are part of the transaction.
     * 
     * Usage:
     * @code
     * ctx.with_tx([&](TxContext& tx) {
     *     tx.db.users().insert(User{"alice"});
     *     tx.db.posts().insert(Post{"hello world"});
     *     // Both inserts commit together
     * });
     * @endcode
     * 
     * @param body Callback to execute within the transaction
     * @return The return value of the callback
     */
    template<typename Func>
    auto with_tx(Func&& body) -> decltype(body(std::declval<TxContext&>())) {
        using ResultType = decltype(body(std::declval<TxContext&>()));
        
        // Start transaction
        int64_t tx_timestamp;
        Status status = ::procedure_start_mut_tx(&tx_timestamp);
        if (is_error(status)) {
            LOG_PANIC("Failed to start transaction");
        }
        
        // Create transaction context
        TxContext tx{Timestamp::from_micros_since_epoch(tx_timestamp)};
        
        // Execute callback
        if constexpr (std::is_void_v<ResultType>) {
            body(tx);
            
            // Commit transaction
            status = ::procedure_commit_mut_tx();
            if (is_error(status)) {
                LOG_PANIC("Failed to commit transaction");
            }
        } else {
            ResultType result = body(tx);
            
            // Commit transaction
            status = ::procedure_commit_mut_tx();
            if (is_error(status)) {
                LOG_PANIC("Failed to commit transaction");
            }
            
            return result;
        }
    }
    
    /**
     * @brief Execute a callback within a database transaction, with explicit rollback control
     * 
     * Similar to with_tx(), but allows the callback to indicate whether to commit or rollback.
     * The callback should return true to commit, false to rollback.
     * 
     * Usage:
     * @code
     * bool success = ctx.try_with_tx([&](TxContext& tx) -> bool {
     *     tx.db.users().insert(User{"alice"});
     *     if (some_condition) {
     *         return false; // Rollback
     *     }
     *     return true; // Commit
     * });
     * @endcode
     * 
     * @param body Callback that returns true to commit, false to rollback
     * @return The return value of the callback
     */
    template<typename Func>
    auto try_with_tx(Func&& body) -> decltype(body(std::declval<TxContext&>())) {
        using ResultType = decltype(body(std::declval<TxContext&>()));
        
        // Start transaction
        int64_t tx_timestamp;
        Status status = ::procedure_start_mut_tx(&tx_timestamp);
        if (is_error(status)) {
            LOG_PANIC("Failed to start transaction");
        }
        
        // Create transaction context
        TxContext tx{Timestamp::from_micros_since_epoch(tx_timestamp)};
        
        // Execute callback
        ResultType result = body(tx);
        
        // For bool results, use the value to decide commit/rollback
        // For other types, always commit (caller can use LOG_PANIC to abort)
        if constexpr (std::is_same_v<ResultType, bool>) {
            if (result) {
                status = ::procedure_commit_mut_tx();
                if (is_error(status)) {
                    LOG_PANIC("Failed to commit transaction");
                }
            } else {
                status = ::procedure_abort_mut_tx();
                if (is_error(status)) {
                    LOG_PANIC("Failed to rollback transaction");
                }
            }
        } else {
            // For non-bool returns, always commit
            status = ::procedure_commit_mut_tx();
            if (is_error(status)) {
                LOG_PANIC("Failed to commit transaction");
            }
        }
        
        return result;
    }
#endif
};

} // namespace SpacetimeDb

#endif // SPACETIMEDB_PROCEDURE_CONTEXT_H
