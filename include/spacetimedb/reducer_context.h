#ifndef REDUCER_CONTEXT_H
#define REDUCER_CONTEXT_H

#include <spacetimedb/bsatn/types.h> // For Identity, ConnectionId
#include <spacetimedb/bsatn/timestamp.h> // For Timestamp
#include <spacetimedb/bsatn/uuid.h> // For Uuid
#include <spacetimedb/random.h> // For StdbRng
#include <spacetimedb/auth_ctx.h> // For AuthCtx
#include <optional>
#include <array>
#include <memory>

// Include database for DatabaseContext
#include <spacetimedb/database.h>

namespace SpacetimeDB {

// Enhanced ReducerContext with database access - matches Rust pattern
struct ReducerContext {
    // Core fields - directly accessible like in Rust
    Identity sender;
    std::optional<ConnectionId> connection_id;
    Timestamp timestamp;
    
    // Database context with name-based access
    DatabaseContext db;
    
private:
    // Authentication context with lazy JWT loading (private like in Rust)
    AuthCtx sender_auth_;
    
    // Lazily initialized RNG (similar to Rust's OnceCell pattern)
    // Using shared_ptr to make ReducerContext copyable
    mutable std::shared_ptr<StdbRng> rng_instance;
    
    // Monotonic counter for UUID v7 generation (31 bits, wraps around)
    mutable uint32_t counter_uuid_ = 0;
    
public:
    // Returns the authorization information for the caller of this reducer
    const AuthCtx& sender_auth() const {
        return sender_auth_;
    }
    
    // Get the random number generator for this reducer call
    // Lazily initialized and seeded with the timestamp
    StdbRng& rng() const {
        if (!rng_instance) {
            rng_instance = std::make_unique<StdbRng>(timestamp);
        }
        return *rng_instance;
    }

    Identity identity() const {
        std::array<uint8_t, 32> buffer;
        ::identity(buffer.data());
        return Identity(buffer);
    }
    
    /**
     * Generate a new random UUID v4.
     * 
     * Creates a random UUID using the reducer's deterministic RNG.
     * 
     * Example:
     * @code
     * SPACETIMEDB_REDUCER(void, create_session, ReducerContext ctx) {
     *     Uuid session_id = ctx.new_uuid_v4();
     *     ctx.db[sessions].insert(Session{session_id});
     * }
     * @endcode
     * 
     * @return A new UUID v4
     */
    Uuid new_uuid_v4() const {
        // Get 16 random bytes from the context RNG
        std::array<uint8_t, 16> random_bytes;
        rng().fill_bytes(random_bytes.data(), 16);
        
        // Generate UUID v4
        return Uuid::from_random_bytes_v4(random_bytes);
    }
    
    /**
     * Generate a new UUID v7.
     * 
     * Creates a time-ordered UUID with the reducer's timestamp, a monotonic counter,
     * and random bytes from the reducer's deterministic RNG.
     * 
     * Example:
     * @code
     * SPACETIMEDB_REDUCER(void, create_user, ReducerContext ctx, std::string name) {
     *     Uuid user_id = ctx.new_uuid_v7();
     *     ctx.db[users].insert(User{user_id, name});
     * }
     * @endcode
     * 
     * @return A new UUID v7
     */
    Uuid new_uuid_v7() const {
        // Get 4 random bytes from the context RNG
        std::array<uint8_t, 4> random_bytes;
        rng().fill_bytes(random_bytes.data(), 4);
        
        // Generate UUID v7 with timestamp and counter
        return Uuid::from_counter_v7(counter_uuid_, timestamp, random_bytes);
    }

    // Constructors
    ReducerContext() : sender_auth_(AuthCtx::internal()) {}
    
    ReducerContext(Identity s, std::optional<ConnectionId> cid, Timestamp ts)
        : sender(s), connection_id(cid), timestamp(ts), 
          sender_auth_(AuthCtx::from_connection_id_opt(cid, s)) {}
};

} // namespace SpacetimeDB

#endif // REDUCER_CONTEXT_H
