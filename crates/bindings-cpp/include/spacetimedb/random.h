#ifndef SPACETIMEDB_RANDOM_H
#define SPACETIMEDB_RANDOM_H

#include <random>
#include <limits>
#include <memory>
#include <spacetimedb/bsatn/timestamp.h>

namespace SpacetimeDb {

// Random number generator for reducer contexts
// Similar to Rust's StdbRng and C#'s Random
// Seeded with the timestamp for deterministic behavior
class StdbRng {
private:
    // Use mt19937_64 for 64-bit Mersenne Twister (same quality as Rust's StdRng)
    mutable std::mt19937_64 engine;
    mutable bool initialized = false;
    Timestamp seed_timestamp;
    
    void ensure_initialized() const {
        if (!initialized) {
            // Seed with timestamp's microseconds since Unix epoch
            // This matches the Rust implementation's approach
            uint64_t seed = static_cast<uint64_t>(seed_timestamp.micros_since_epoch());
            engine.seed(seed);
            initialized = true;
        }
    }
    
public:
    // Constructor that takes a timestamp for seeding
    explicit StdbRng(Timestamp ts) : seed_timestamp(ts) {}
    
    // Generate a random 32-bit unsigned integer
    uint32_t next_u32() const {
        ensure_initialized();
        return static_cast<uint32_t>(engine() & 0xFFFFFFFF);
    }
    
    // Generate a random 64-bit unsigned integer
    uint64_t next_u64() const {
        ensure_initialized();
        return engine();
    }
    
    // Generate a random integer in the range [min, max]
    template<typename T>
    T gen_range(T min, T max) const {
        ensure_initialized();
        if constexpr (std::is_integral_v<T>) {
            std::uniform_int_distribution<T> dist(min, max);
            return dist(engine);
        } else {
            std::uniform_real_distribution<T> dist(min, max);
            return dist(engine);
        }
    }
    
    // Generate a random integer of type T
    template<typename T>
    T gen() const {
        ensure_initialized();
        if constexpr (std::is_same_v<T, bool>) {
            return engine() & 1;
        } else if constexpr (std::is_integral_v<T>) {
            if constexpr (std::is_unsigned_v<T>) {
                if constexpr (sizeof(T) <= 4) {
                    return static_cast<T>(next_u32());
                } else {
                    return static_cast<T>(next_u64());
                }
            } else {
                // For signed types, use the full range
                std::uniform_int_distribution<T> dist(
                    std::numeric_limits<T>::min(),
                    std::numeric_limits<T>::max()
                );
                return dist(engine);
            }
        } else if constexpr (std::is_floating_point_v<T>) {
            // Generate float in [0, 1)
            if constexpr (std::is_same_v<T, float>) {
                return static_cast<float>(next_u32()) / static_cast<float>(UINT32_MAX);
            } else {
                return static_cast<double>(next_u64()) / static_cast<double>(UINT64_MAX);
            }
        }
    }
    
    // Fill a buffer with random bytes
    void fill_bytes(uint8_t* dest, size_t count) const {
        ensure_initialized();
        for (size_t i = 0; i < count; i++) {
            dest[i] = static_cast<uint8_t>(engine() & 0xFF);
        }
    }
    
    // Fill a vector with random bytes
    void fill_bytes(std::vector<uint8_t>& dest) const {
        fill_bytes(dest.data(), dest.size());
    }
    
    // Generate a random float in [0, 1)
    float gen_float() const {
        return gen<float>();
    }
    
    // Generate a random double in [0, 1)
    double gen_double() const {
        return gen<double>();
    }
    
    // Generate a random boolean
    bool gen_bool() const {
        return gen<bool>();
    }
    
    // Shuffle a container randomly
    template<typename RandomIt>
    void shuffle(RandomIt first, RandomIt last) const {
        ensure_initialized();
        std::shuffle(first, last, engine);
    }
    
    // Sample a random element from a container
    template<typename Container>
    auto sample(const Container& container) const -> decltype(container[0]) {
        ensure_initialized();
        if (container.empty()) {
            // Return first element for empty container (undefined behavior)
            // In production, you should check container size before calling sample
            return container[0];
        }
        std::uniform_int_distribution<size_t> dist(0, container.size() - 1);
        return container[dist(engine)];
    }
};

} // namespace SpacetimeDb

#endif // SPACETIMEDB_RANDOM_H