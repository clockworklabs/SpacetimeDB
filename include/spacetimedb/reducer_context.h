#ifndef REDUCER_CONTEXT_H
#define REDUCER_CONTEXT_H

#include <spacetimedb/bsatn/types.h> // For Identity, ConnectionId
#include <spacetimedb/bsatn/timestamp.h> // For Timestamp
#include <spacetimedb/random.h> // For StdbRng
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
    // Lazily initialized RNG (similar to Rust's OnceCell pattern)
    // Using shared_ptr to make ReducerContext copyable
    mutable std::shared_ptr<StdbRng> rng_instance;
    
public:
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
        
        // Reverse the bytes to convert from little-endian to big-endian
        std::reverse(buffer.begin(), buffer.end());
        
        // Use constructor instead of from_byte_array
        return Identity(buffer);
    }

    // Convenience method to generate a random value (similar to Rust's ctx.random())
    template<typename T>
    T random() const {
        return rng().gen<T>();
    }
    
    // Constructors
    ReducerContext() = default;
    
    ReducerContext(Identity s, std::optional<ConnectionId> cid, Timestamp ts)
        : sender(s), connection_id(cid), timestamp(ts) {}
};

} // namespace SpacetimeDB

#endif // REDUCER_CONTEXT_H
