#ifndef SPACETIMEDB_VIEW_CONTEXT_H
#define SPACETIMEDB_VIEW_CONTEXT_H

#include <spacetimedb/bsatn/types.h> // For Identity
#include <spacetimedb/bsatn/timestamp.h> // For Timestamp
#include <spacetimedb/readonly_database_context.h> // For ReadOnlyDatabaseContext
#include <array>

namespace SpacetimeDB {

/**
 * @brief Context for views with caller identity
 * 
 * ViewContext provides read-only database access along with the identity
 * of the caller who invoked the view. Use this when the view needs to
 * filter or customize results based on who is calling it.
 * 
 * Key differences from ReducerContext:
 * - db is ReadOnlyDatabaseContext (no mutations allowed)
 * - No connection_id (views are stateless, don't track connections)
 * - No rng() method (views should be deterministic)
 * 
 * Example usage:
 * @code
 * SPACETIMEDB_VIEW(std::vector<Item>, get_my_items, Public, ViewContext ctx) {
 *     std::vector<Item> my_items;
 *     // Filter by caller's identity using indexed field
 *     for (const auto& item : ctx.db[item_owner].filter(ctx.sender)) {
 *         my_items.push_back(item);
 *     }
 *     return Ok(my_items);
 * }
 * @endcode
 */
struct ViewContext {
    // Caller's identity - who invoked this view
    Identity sender;
    
    // Read-only database access - no mutations allowed
    ReadOnlyDatabaseContext db;
    
    // Constructors
    ViewContext() = default;
    
    explicit ViewContext(Identity s)
        : sender(s) {}
};

/**
 * @brief Context for anonymous views without caller identity
 * 
 * AnonymousViewContext provides read-only database access without
 * exposing the caller's identity. Use this for views that return
 * the same data regardless of who calls them.
 * 
 * This is more efficient than ViewContext as it doesn't require
 * identity information to be passed from the host.
 * 
 * Key differences from ViewContext:
 * - No sender field (caller identity not available)
 * - Otherwise identical functionality
 * 
 * Example usage:
 * @code
 * SPACETIMEDB_VIEW(std::optional<uint64_t>, count_users, Public, AnonymousViewContext ctx) {
 *     return Ok(std::optional<uint64_t>(ctx.db[user].count()));
 * }
 * @endcode
 */
struct AnonymousViewContext {
    // Read-only database access - no mutations allowed
    ReadOnlyDatabaseContext db;
    
    // Constructors
    AnonymousViewContext() = default;
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_VIEW_CONTEXT_H
