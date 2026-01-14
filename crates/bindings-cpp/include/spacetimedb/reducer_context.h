#ifndef REDUCER_CONTEXT_H
#define REDUCER_CONTEXT_H

#include <spacetimedb/bsatn/types.h> // For Identity, ConnectionId
#include <spacetimedb/bsatn/timestamp.h> // For Timestamp
#include <spacetimedb/random.h> // For StdbRng
#include <spacetimedb/auth_ctx.h> // For AuthCtx
#include <optional>
#include <array>
#include <memory>

// Include database for DatabaseContext
#include <spacetimedb/database.h>

namespace SpacetimeDb {

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

    // Constructors
    ReducerContext() : sender_auth_(AuthCtx::Internal()) {}
    
    ReducerContext(Identity s, std::optional<ConnectionId> cid, Timestamp ts)
        : sender(s), connection_id(cid), timestamp(ts), 
          sender_auth_(AuthCtx::FromConnectionIdOpt(cid, s)) {}
};

} // namespace SpacetimeDB

#endif // REDUCER_CONTEXT_H
