#pragma once

#include <cstdint>
#include <array>
#include <string>
#include <optional>
#include <cstring>
#include <sstream>
#include <iomanip>
#include "types.h"
#include "timestamp.h"

// Forward declarations
namespace SpacetimeDB {
namespace bsatn {
    class Writer;
    class Reader;
}

/**
 * A universally unique identifier (UUID).
 * 
 * Supports UUID Nil, Max, V4 (random), and V7 (timestamp + counter + random).
 * 
 * This type corresponds to SpacetimeDB's Uuid special type, represented as a
 * product type with a single __uuid__ field containing a u128 value.
 */
class Uuid {
public:
    /// UUID version enumeration
    enum class Version {
        Nil,   ///< The "nil" UUID (all zeros)
        V4,    ///< Version 4: Random
        V7,    ///< Version 7: Timestamp + counter + random
        Max    ///< The "max" UUID (all ones)
    };

private:
    u128 __uuid__;  // Internal representation matches SpacetimeDB special type tag

public:
    // =========================================================================
    // Constructors
    // =========================================================================
    
    /// Default constructor creates NIL UUID
    Uuid() : __uuid__(0, 0) {}
    
    /// Construct from u128 value
    explicit Uuid(u128 value) : __uuid__(value) {}
    
    /// Construct from high and low 64-bit values
    Uuid(uint64_t high, uint64_t low) : __uuid__(high, low) {}
    
    // =========================================================================
    // Constants
    // =========================================================================
    
    // =========================================================================
    // Constants
    // =========================================================================
    
    /// The nil UUID (all zeros)
    static Uuid nil() {
        return Uuid(0, 0);
    }
    
    /// The max UUID (all ones)
    static Uuid max() {
        return Uuid(0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF);
    }
    
    // =========================================================================
    // Factory Methods
    // =========================================================================
    
    /// Create a UUID from a u128 value
    static Uuid from_u128(u128 value) {
        return Uuid(value);
    }
    
    /// Create a UUID from high and low 64-bit values
    static Uuid from_u64(uint64_t high, uint64_t low) {
        return Uuid(high, low);
    }
    
    /**
     * Create a UUID v4 from explicit random bytes.
     * 
     * This method assumes the bytes are already sufficiently random.
     * It only sets the appropriate bits for the UUID version and variant.
     * 
     * @param random_bytes Exactly 16 random bytes
     * @return A UUID v4
     */
    static Uuid from_random_bytes_v4(const std::array<uint8_t, 16>& random_bytes) {
        std::array<uint8_t, 16> bytes = random_bytes;
        
        // Set version bits (version 4)
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        
        // Set variant bits (RFC 4122)
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        
        return from_bytes_be(bytes);
    }
    
    /**
     * Generate a UUID v7 using a monotonic counter, a timestamp, and random bytes.
     * 
     * The UUID v7 is structured as follows:
     * ```
     * ┌───────────────────────────────────────────────┬───────────────────┐
     * | B0  | B1  | B2  | B3  | B4  | B5              |         B6        |
     * ├───────────────────────────────────────────────┼───────────────────┤
     * |                 unix_ts_ms                    |      version 7    |
     * └───────────────────────────────────────────────┴───────────────────┘
     * ┌──────────────┬─────────┬──────────────────┬───────────────────────┐
     * | B7           | B8      | B9  | B10 | B11  | B12 | B13 | B14 | B15 |
     * ├──────────────┼─────────┼──────────────────┼───────────────────────┤
     * | counter_high | variant |    counter_low   |        random         |
     * └──────────────┴─────────┴──────────────────┴───────────────────────┘
     * ```
     * 
     * @param counter Reference to monotonic counter (31 bits, wraps around)
     * @param now Current timestamp
     * @param random_bytes Exactly 4 random bytes for entropy
     * @return A UUID v7
     */
    static Uuid from_counter_v7(uint32_t& counter, const Timestamp& now, const std::array<uint8_t, 4>& random_bytes) {
        // Get timestamp in milliseconds since Unix epoch
        int64_t ts_ms = now.millis_since_epoch();
        
        // Monotonic counter value (31 bits) - wrap around on overflow
        uint32_t counter_val = counter;
        counter = (counter + 1) & 0x7FFF'FFFF;
        
        std::array<uint8_t, 16> bytes = {0};
        
        // unix_ts_ms (48 bits, big-endian)
        bytes[0] = static_cast<uint8_t>((ts_ms >> 40) & 0xFF);
        bytes[1] = static_cast<uint8_t>((ts_ms >> 32) & 0xFF);
        bytes[2] = static_cast<uint8_t>((ts_ms >> 24) & 0xFF);
        bytes[3] = static_cast<uint8_t>((ts_ms >> 16) & 0xFF);
        bytes[4] = static_cast<uint8_t>((ts_ms >> 8) & 0xFF);
        bytes[5] = static_cast<uint8_t>(ts_ms & 0xFF);
        
        // Version 7 (4 bits in high nibble of byte 6)
        bytes[6] = 0x70;
        
        // Counter bits (31 bits split across bytes 7, 9, 10, 11)
        bytes[7] = static_cast<uint8_t>((counter_val >> 23) & 0xFF);
        bytes[9] = static_cast<uint8_t>((counter_val >> 15) & 0xFF);
        bytes[10] = static_cast<uint8_t>((counter_val >> 7) & 0xFF);
        bytes[11] = static_cast<uint8_t>((counter_val & 0x7F) << 1);
        
        // Variant (RFC 4122, 2 bits in high bits of byte 8)
        bytes[8] = 0x80;
        
        // Random bytes (32 bits)
        bytes[12] |= random_bytes[0] & 0x7F;
        bytes[13] = random_bytes[1];
        bytes[14] = random_bytes[2];
        bytes[15] = random_bytes[3];
        
        return from_bytes_be(bytes);
    }
    
    /**
     * Parse a UUID from a string representation.
     * 
     * Supports standard hyphenated format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
     * Both uppercase and lowercase hex digits are accepted.
     * 
     * @param s UUID string
     * @return Parsed UUID, or std::nullopt if parsing fails
     */
    static std::optional<Uuid> parse_str(const std::string& s) {
        // Expected format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx (36 characters)
        if (s.length() != 36) {
            return std::nullopt;
        }
        
        // Check hyphens are in the right positions
        if (s[8] != '-' || s[13] != '-' || s[18] != '-' || s[23] != '-') {
            return std::nullopt;
        }
        
        std::array<uint8_t, 16> bytes;
        int byte_idx = 0;
        
        // Parse each hex pair
        for (size_t i = 0; i < s.length() && byte_idx < 16; ++i) {
            if (s[i] == '-') continue;
            
            // Parse two hex digits
            if (i + 1 >= s.length()) {
                return std::nullopt;
            }
            
            char high_char = s[i];
            char low_char = s[i + 1];
            
            auto hex_to_int = [](char c) -> int {
                if (c >= '0' && c <= '9') return c - '0';
                if (c >= 'a' && c <= 'f') return c - 'a' + 10;
                if (c >= 'A' && c <= 'F') return c - 'A' + 10;
                return -1;
            };
            
            int high = hex_to_int(high_char);
            int low = hex_to_int(low_char);
            
            if (high < 0 || low < 0) {
                return std::nullopt;
            }
            
            bytes[byte_idx++] = static_cast<uint8_t>((high << 4) | low);
            ++i; // Skip the second character of the pair
        }
        
        if (byte_idx != 16) {
            return std::nullopt;
        }
        
        return from_bytes_be(bytes);
    }
    
    // =========================================================================
    // Accessors
    // =========================================================================
    
    /// Get the underlying u128 value
    u128 as_u128() const {
        return __uuid__;
    }
    
    /**
     * Get the version of this UUID.
     * 
     * @return UUID version, or std::nullopt if version is not recognized
     */
    std::optional<Version> get_version() const {
        // Special cases for NIL and MAX
        if (*this == nil()) return Version::Nil;
        if (*this == max()) return Version::Max;
        
        // Extract version from byte 6 (high nibble)
        std::array<uint8_t, 16> bytes = to_bytes_be();
        uint8_t version = (bytes[6] >> 4) & 0x0F;
        
        switch (version) {
            case 4: return Version::V4;
            case 7: return Version::V7;
            default: return std::nullopt;
        }
    }
    
    /**
     * Extract the 31-bit monotonic counter from a UUID v7.
     * 
     * Intended for testing and debugging.
     * 
     * @return The counter value (0 to 2^31-1)
     */
    int32_t get_counter() const {
        std::array<uint8_t, 16> bytes = to_bytes_be();
        
        uint32_t high = bytes[7];        // bits 30..23
        uint32_t mid1 = bytes[9];        // bits 22..15
        uint32_t mid2 = bytes[10];       // bits 14..7
        uint32_t low = bytes[11] >> 1;   // bits 6..0
        
        // Reconstruct 31-bit counter
        return static_cast<int32_t>((high << 23) | (mid1 << 15) | (mid2 << 7) | low);
    }
    
    // =========================================================================
    // String Conversion
    // =========================================================================
    
    /**
     * Convert UUID to standard string representation.
     * 
     * Format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx (lowercase hex)
     * 
     * @return UUID string
     */
    std::string to_string() const {
        std::array<uint8_t, 16> bytes = to_bytes_be();
        
        std::ostringstream oss;
        oss << std::hex << std::setfill('0');
        
        // xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        for (int i = 0; i < 16; ++i) {
            oss << std::setw(2) << static_cast<unsigned>(bytes[i]);
            if (i == 3 || i == 5 || i == 7 || i == 9) {
                oss << '-';
            }
        }
        
        return oss.str();
    }
    
    // =========================================================================
    // Comparison Operators
    // =========================================================================
    
    bool operator==(const Uuid& other) const {
        return __uuid__ == other.__uuid__;
    }
    
    bool operator!=(const Uuid& other) const {
        return __uuid__ != other.__uuid__;
    }
    
    bool operator<(const Uuid& other) const {
        // Compare as unsigned integers
        if (__uuid__.high != other.__uuid__.high) {
            return __uuid__.high < other.__uuid__.high;
        }
        return __uuid__.low < other.__uuid__.low;
    }
    
    bool operator<=(const Uuid& other) const {
        return *this < other || *this == other;
    }
    
    bool operator>(const Uuid& other) const {
        return !(*this <= other);
    }
    
    bool operator>=(const Uuid& other) const {
        return !(*this < other);
    }
    
    // =========================================================================
    // BSATN Serialization
    // =========================================================================
    
    void bsatn_serialize(SpacetimeDB::bsatn::Writer& writer) const;
    static Uuid bsatn_deserialize(SpacetimeDB::bsatn::Reader& reader);

private:
    // =========================================================================
    // Internal Helpers
    // =========================================================================
    
    /// Convert UUID to big-endian byte array
    std::array<uint8_t, 16> to_bytes_be() const {
        std::array<uint8_t, 16> bytes;
        
        // u128 is stored in native byte order, convert to big-endian
        // Big-endian: most significant byte first
        // high comes first (bytes 0-7), then low (bytes 8-15)
        for (int i = 0; i < 8; ++i) {
            bytes[7 - i] = static_cast<uint8_t>((__uuid__.high >> (i * 8)) & 0xFF);
        }
        for (int i = 0; i < 8; ++i) {
            bytes[15 - i] = static_cast<uint8_t>((__uuid__.low >> (i * 8)) & 0xFF);
        }
        
        return bytes;
    }
    
    /// Create UUID from big-endian byte array
    static Uuid from_bytes_be(const std::array<uint8_t, 16>& bytes) {
        u128 value;
        
        // Convert big-endian bytes to u128
        value.low = 0;
        value.high = 0;
        
        // high comes from bytes 0-7, low from bytes 8-15
        for (int i = 0; i < 8; ++i) {
            value.high |= static_cast<uint64_t>(bytes[7 - i]) << (i * 8);
        }
        for (int i = 0; i < 8; ++i) {
            value.low |= static_cast<uint64_t>(bytes[15 - i]) << (i * 8);
        }
        
        return Uuid(value);
    }
};

} // namespace SpacetimeDB

// =============================================================================
// BSATN Implementation
// =============================================================================

#include "writer.h"
#include "reader.h"
#include "traits.h"
#include "algebraic_type.h"

namespace SpacetimeDB {

// Uuid BSATN implementation
// UUIDs are serialized as a product with a single __uuid__ field containing a u128
inline void Uuid::bsatn_serialize(SpacetimeDB::bsatn::Writer& writer) const {
    // Serialize the u128 value directly (little-endian)
    writer.write_u64_le(__uuid__.low);
    writer.write_u64_le(__uuid__.high);
}

inline Uuid Uuid::bsatn_deserialize(SpacetimeDB::bsatn::Reader& reader) {
    // Deserialize the u128 value directly (little-endian)
    uint64_t low = reader.read_u64_le();
    uint64_t high = reader.read_u64_le();
    return Uuid(high, low);
}

} // namespace SpacetimeDB

// Note: bsatn_traits specialization for Uuid is defined in type_extensions.h
// to ensure consistent handling with other special types
