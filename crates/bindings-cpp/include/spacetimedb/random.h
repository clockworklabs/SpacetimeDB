#ifndef SPACETIMEDB_RANDOM_H
#define SPACETIMEDB_RANDOM_H

#include <random>
#include <limits>
#include <memory>
#include <spacetimedb/bsatn/timestamp.h>

namespace SpacetimeDB {

/**
 * @brief Deterministic random number generator for SpacetimeDB reducers
 * 
 * StdbRng provides a cryptographically-strong random number generator that is
 * seeded with the reducer's timestamp, ensuring:
 * - **Deterministic** behavior: Same inputs always produce same random sequence
 * - **Reproducible** tests: Reducer execution can be replayed exactly
 * - **Consensus-safe**: All nodes generate identical random values
 * 
 * The RNG uses the Mersenne Twister algorithm (std::mt19937_64) seeded with
 * the reducer's timestamp in microseconds since Unix epoch.
 * 
 * @warning **DO NOT use for cryptographic purposes!**
 * This RNG is deterministic and predictable. For security-sensitive operations,
 * use a cryptographic RNG from a trusted source.
 * 
 * @example Basic usage in a reducer:
 * @code
 * SPACETIMEDB_REDUCER(void, spawn_enemy, ReducerContext ctx) {
 *     // Get the deterministic RNG for this reducer call
 *     auto& rng = ctx.rng();
 *     
 *     // Generate random enemy stats
 *     uint32_t health = rng.gen_range(50u, 100u);
 *     uint32_t attack = rng.gen_range(10u, 25u);
 *     float speed = rng.gen_range(1.0f, 3.0f);
 *     
 *     ctx.db[enemies].insert(Enemy{health, attack, speed});
 * }
 * @endcode
 * 
 * @note The RNG is lazily initialized on first use to avoid overhead for reducers
 *       that don't need random numbers
 * 
 * @ingroup sdk_runtime
 */
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
    
    /**
     * @brief Generate a random 32-bit unsigned integer
     * 
     * Produces a uniformly distributed random value across the full uint32_t range [0, 2^32-1].
     * 
     * @return Random uint32_t value
     * 
     * @example
     * @code
     * auto& rng = ctx.rng();
     * uint32_t dice_roll = (rng.next_u32() % 6) + 1;  // 1-6
     * uint32_t random_id = rng.next_u32();  // Full range
     * @endcode
     */
    uint32_t next_u32() const {
        ensure_initialized();
        return static_cast<uint32_t>(engine() & 0xFFFFFFFF);
    }
    
    /**
     * @brief Generate a random 64-bit unsigned integer
     * 
     * Produces a uniformly distributed random value across the full uint64_t range [0, 2^64-1].
     * 
     * @return Random uint64_t value
     * 
     * @example
     * @code
     * auto& rng = ctx.rng();
     * uint64_t large_random = rng.next_u64();
     * uint64_t timestamp_noise = rng.next_u64() % 1000;  // 0-999
     * @endcode
     */
    uint64_t next_u64() const {
        ensure_initialized();
        return engine();
    }
    
    /**
     * @brief Generate a random value in the specified range [min, max]
     * 
     * Produces a uniformly distributed random value within the inclusive range [min, max].
     * Works with both integral and floating-point types.
     * 
     * @tparam T Type of the range bounds (integral or floating-point)
     * @param min Lower bound (inclusive)
     * @param max Upper bound (inclusive)
     * @return Random value in [min, max]
     * 
     * @example Integer ranges:
     * @code
     * auto& rng = ctx.rng();
     * 
     * // Dice rolls
     * int d6 = rng.gen_range(1, 6);      // 1-6 inclusive
     * int d20 = rng.gen_range(1, 20);    // 1-20 inclusive
     * 
     * // Game stats
     * uint32_t damage = rng.gen_range(10u, 50u);  // 10-50 damage
     * int temperature = rng.gen_range(-10, 35);   // -10°C to 35°C
     * @endcode
     * 
     * @example Floating-point ranges:
     * @code
     * // Spawn position in game world
     * float x = rng.gen_range(0.0f, 100.0f);
     * float y = rng.gen_range(0.0f, 100.0f);
     * 
     * // Physics simulation
     * double velocity = rng.gen_range(0.5, 2.5);
     * float friction = rng.gen_range(0.1f, 0.9f);
     * @endcode
     * 
     * @note For integral types, both bounds are inclusive
     * @note For floating-point types, the distribution is continuous in [min, max]
     */
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
    
    /**
     * @brief Fill a buffer with random bytes
     * 
     * Generates random bytes and writes them to the provided buffer.
     * Each byte is uniformly distributed in [0, 255].
     * 
     * @param dest Pointer to destination buffer
     * @param count Number of bytes to generate
     * 
     * @example Generate random data for UUID:
     * @code
     * std::array<uint8_t, 16> uuid_bytes;
     * ctx.rng().fill_bytes(uuid_bytes.data(), 16);
     * Uuid id = Uuid::from_random_bytes_v4(uuid_bytes);
     * @endcode
     * 
     * @example Generate random salt:
     * @code
     * uint8_t salt[32];
     * ctx.rng().fill_bytes(salt, 32);
     * @endcode
     * 
     * @warning This is NOT cryptographically secure! Do not use for
     *          security-sensitive operations like password hashing or encryption keys.
     */
    void fill_bytes(uint8_t* dest, size_t count) const {
        ensure_initialized();
        for (size_t i = 0; i < count; i++) {
            dest[i] = static_cast<uint8_t>(engine() & 0xFF);
        }
    }
    
    /**
     * @brief Fill a vector with random bytes
     * 
     * Convenience overload that fills an existing std::vector<uint8_t> with random data.
     * The vector size must be pre-allocated.
     * 
     * @param dest Vector to fill (must be pre-sized)
     * 
     * @example
     * @code
     * std::vector<uint8_t> random_data(256);  // Pre-allocate 256 bytes
     * ctx.rng().fill_bytes(random_data);      // Fill with random values
     * @endcode
     */
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
    
    /**
     * @brief Randomly shuffle a container using Fisher-Yates algorithm
     * 
     * Performs an in-place random permutation of the elements in [first, last).
     * Each permutation is equally likely (uniform distribution).
     * 
     * @tparam RandomIt Random access iterator type
     * @param first Iterator to the beginning of the range
     * @param last Iterator to the end of the range
     * 
     * @example Shuffle a deck of cards:
     * @code
     * std::vector<Card> deck = create_deck();
     * ctx.rng().shuffle(deck.begin(), deck.end());
     * // deck is now randomly permuted
     * @endcode
     * 
     * @example Randomize player turn order:
     * @code
     * std::vector<PlayerId> players = {1, 2, 3, 4};
     * ctx.rng().shuffle(players.begin(), players.end());
     * for (auto player_id : players) {
     *     // Process players in random order
     * }
     * @endcode
     */
    template<typename RandomIt>
    void shuffle(RandomIt first, RandomIt last) const {
        ensure_initialized();
        std::shuffle(first, last, engine);
    }
    
    /**
     * @brief Select a random element from a container
     * 
     * Returns a random element from the container with uniform probability.
     * Each element has an equal chance of being selected.
     * 
     * @tparam Container Container type (must support operator[] and size())
     * @param container The container to sample from
     * @return Random element from the container
     * 
     * @example Select random enemy type:
     * @code
     * std::vector<std::string> enemy_types = {"goblin", "orc", "troll", "dragon"};
     * std::string enemy = ctx.rng().sample(enemy_types);
     * LOG_INFO("Spawned: " + enemy);
     * @endcode
     * 
     * @example Random loot drop:
     * @code
     * std::vector<Item> loot_table = {common_item, rare_item, epic_item};
     * Item dropped = ctx.rng().sample(loot_table);
     * @endcode
     * 
     * @warning Undefined behavior if container is empty! Check size before calling.
     * @note Returns by value (copy) - use references in container if needed
     */
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

} // namespace SpacetimeDB

#endif // SPACETIMEDB_RANDOM_H