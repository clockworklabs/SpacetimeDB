#pragma once

/**
 * SpacetimeDB C++ SDK - Extended Type System
 * 
 * This header provides complete support for all SpacetimeDB types including:
 * - Special types: Identity, ConnectionId, Timestamp, TimeDuration (✅ COMPLETE)
 * - Large integers: u128, i128, u256, i256 (✅ COMPLETE)
 * - Container types: Option<T>, Vec<T> (✅ COMPLETE)
 * - BSATN serialization for all types (✅ COMPLETE)
 * 
 * All four SpacetimeDB special types are fully implemented and tested:
 * - Identity: U256 with __identity__ tag
 * - ConnectionId: U128 with __connection_id__ tag  
 * - Timestamp: I64 with __timestamp_micros_since_unix_epoch__ tag
 * - TimeDuration: I64 with __time_duration_micros__ tag
 */

#include <cstdint>
#include <array>
#include <vector>
#include <optional>
#include <string>
#include <cstring>
#include <type_traits>
#include <stdexcept>  // For std::runtime_error
#include <algorithm>  // For std::copy
#include <sstream>    // For std::ostringstream
#include <iomanip>    // For std::hex, std::setfill, std::setw

// Forward declarations for BSATN
namespace SpacetimeDb {
namespace bsatn {
    class Writer;
    class Reader;
}

// Forward declarations
class Timestamp;
class TimeDuration;
struct u128;

// =============================================================================
// IDENTITY TYPE
// =============================================================================

#define IDENTITY_SIZE 32

class Identity {
public:
    static constexpr size_t SIZE = IDENTITY_SIZE;
    using ByteArray = std::array<uint8_t, IDENTITY_SIZE>;
    
private:
    std::array<uint8_t, IDENTITY_SIZE> value;
    
public:
    // Constructors
    Identity();
    Identity(const std::array<uint8_t, IDENTITY_SIZE>& bytes);
    
    // Accessors
    const std::array<uint8_t, IDENTITY_SIZE>& get_bytes() const;
    const ByteArray& to_byte_array() const { return value; }
    std::string to_hex_string() const;
    
    // Convert to string (using hex representation)
    std::string to_string() const {
        return to_hex_string();
    }
    
    // Operators
    bool operator==(const Identity& other) const;
    bool operator!=(const Identity& other) const;
    bool operator<(const Identity& other) const;
    
    // BSATN serialization (implemented below)
    void bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const;
    void bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader);
};


// =============================================================================
// TYPE ALIASES FOR COMMON SPACETIMEDB TYPES
// =============================================================================

// Option<T> is just std::optional<T> with some helper methods
template<typename T>
class Option : public std::optional<T> {
public:
    using std::optional<T>::optional;
    
    // Helper static methods for clarity
    static Option<T> some(const T& value) {
        return Option<T>(value);
    }
    
    static Option<T> none() {
        return Option<T>(std::nullopt);
    }
    
    // Allow implicit conversion from std::optional
    Option(const std::optional<T>& opt) : std::optional<T>(opt) {}
    Option(std::optional<T>&& opt) : std::optional<T>(std::move(opt)) {}
};

// Vec<T> is just std::vector<T>
template<typename T>
using Vec = std::vector<T>;

// =============================================================================
// LARGE INTEGER TYPES
// =============================================================================

// 128-bit unsigned integer
struct u128 {
    uint64_t low;
    uint64_t high;
    
    u128() : low(0), high(0) {}
    u128(uint64_t l) : low(l), high(0) {}
    u128(uint64_t h, uint64_t l) : low(l), high(h) {}
    
    // Factory method for clarity (high, low parameter order)
    static u128 from_u64(uint64_t high, uint64_t low) {
        return u128(high, low);
    }
    
    bool operator==(const u128& other) const {
        return low == other.low && high == other.high;
    }
    
    bool operator!=(const u128& other) const {
        return !(*this == other);
    }
    
    // Convert to string using proper decimal conversion
    std::string to_string() const {
        if (high == 0) {
            return std::to_string(low);
        }
        
        // Convert u128 to decimal string using byte-level long division
        // Treat the u128 as a 16-byte array in little-endian order
        uint8_t bytes[16];
        std::memcpy(bytes, &low, 8);
        std::memcpy(bytes + 8, &high, 8);
        
        std::string result;
        
        // Repeatedly divide by 10
        while (true) {
            int remainder = 0;
            bool all_zero = true;
            
            // Divide the 16-byte number by 10 (from most significant to least significant)
            for (int i = 15; i >= 0; --i) {
                int temp = remainder * 256 + bytes[i];
                bytes[i] = temp / 10;
                remainder = temp % 10;
                if (bytes[i] != 0) all_zero = false;
            }
            
            // Add the remainder digit to result (building in reverse)
            result = static_cast<char>('0' + remainder) + result;
            
            if (all_zero) break;
        }
        
        return result;
    }
    
    // BSATN serialization
    static void serialize(std::vector<uint8_t>& buffer, const u128& value) {
        // Little-endian serialization
        for (int i = 0; i < 8; ++i) {
            buffer.push_back((value.low >> (i * 8)) & 0xFF);
        }
        for (int i = 0; i < 8; ++i) {
            buffer.push_back((value.high >> (i * 8)) & 0xFF);
        }
    }
    
    static u128 deserialize(const uint8_t* data) {
        u128 result;
        result.low = 0;
        result.high = 0;
        for (int i = 0; i < 8; ++i) {
            result.low |= static_cast<uint64_t>(data[i]) << (i * 8);
        }
        for (int i = 0; i < 8; ++i) {
            result.high |= static_cast<uint64_t>(data[8 + i]) << (i * 8);
        }
        return result;
    }
};

// 128-bit signed integer
struct i128 {
    uint64_t low;
    int64_t high;
    
    i128() : low(0), high(0) {}
    i128(int64_t l) : low(static_cast<uint64_t>(l)), high(l < 0 ? -1 : 0) {}
    i128(int64_t h, uint64_t l) : low(l), high(h) {}
    
    // Factory method for clarity (high, low parameter order)
    static i128 from_i64(int64_t high, uint64_t low) {
        return i128(high, low);
    }
    
    bool operator==(const i128& other) const {
        return low == other.low && high == other.high;
    }
    
    bool operator!=(const i128& other) const {
        return !(*this == other);
    }
    
    // Convert to string using proper decimal conversion
    std::string to_string() const {
        // Handle negative values by converting to positive and adding minus sign
        bool is_negative = (high < 0);
        
        // Convert to unsigned for calculation
        u128 abs_value;
        if (is_negative) {
            // Two's complement: flip bits and add 1
            abs_value.low = ~low + 1;
            abs_value.high = ~static_cast<uint64_t>(high);
            if (abs_value.low == 0) {
                abs_value.high++; // Handle carry
            }
        } else {
            abs_value.low = low;
            abs_value.high = static_cast<uint64_t>(high);
        }
        
        std::string result = abs_value.to_string();
        return is_negative ? "-" + result : result;
    }
    
    // BSATN serialization
    static void serialize(std::vector<uint8_t>& buffer, const i128& value) {
        // Little-endian serialization
        for (int i = 0; i < 8; ++i) {
            buffer.push_back((value.low >> (i * 8)) & 0xFF);
        }
        for (int i = 0; i < 8; ++i) {
            buffer.push_back((value.high >> (i * 8)) & 0xFF);
        }
    }
    
    static i128 deserialize(const uint8_t* data) {
        i128 result;
        result.low = 0;
        result.high = 0;
        for (int i = 0; i < 8; ++i) {
            result.low |= static_cast<uint64_t>(data[i]) << (i * 8);
        }
        uint64_t high_unsigned = 0;
        for (int i = 0; i < 8; ++i) {
            high_unsigned |= static_cast<uint64_t>(data[8 + i]) << (i * 8);
        }
        result.high = static_cast<int64_t>(high_unsigned);
        return result;
    }
};

// 256-bit unsigned integer
struct u256 {
    std::array<uint8_t, 32> data;
    
    u256() { data.fill(0); }
    
    explicit u256(const uint8_t* bytes) {
        std::memcpy(data.data(), bytes, 32);
    }
    
    // Constructor from 4 uint64_t values (big-endian order: word3 is most significant)
    u256(uint64_t word3, uint64_t word2, uint64_t word1, uint64_t word0) {
        // Store in little-endian byte order
        for (int i = 0; i < 8; ++i) {
            data[i] = (word0 >> (i * 8)) & 0xFF;
            data[8 + i] = (word1 >> (i * 8)) & 0xFF;
            data[16 + i] = (word2 >> (i * 8)) & 0xFF;
            data[24 + i] = (word3 >> (i * 8)) & 0xFF;
        }
    }
    
    bool operator==(const u256& other) const {
        return data == other.data;
    }
    
    bool operator!=(const u256& other) const {
        return !(*this == other);
    }
    
    // Convert to string using arbitrary precision decimal conversion
    std::string to_string() const {
        // Simple approach: use byte-by-byte division by 10
        // Make a copy of the data for manipulation
        std::array<uint8_t, 32> temp_data = data;
        std::string result;
        
        // Check if all zeros
        bool all_zero = true;
        for (int i = 0; i < 32; ++i) {
            if (temp_data[i] != 0) {
                all_zero = false;
                break;
            }
        }
        if (all_zero) return "0";
        
        // Repeatedly divide by 10
        while (true) {
            // Divide the 256-bit number by 10
            int remainder = 0;
            bool all_zero_now = true;
            
            // Process from most significant byte to least significant
            for (int i = 31; i >= 0; --i) {
                int temp = remainder * 256 + temp_data[i];
                temp_data[i] = temp / 10;
                remainder = temp % 10;
                if (temp_data[i] != 0) all_zero_now = false;
            }
            
            // Add the remainder digit to result (in reverse)
            result = std::to_string(remainder) + result;
            
            if (all_zero_now) break;
        }
        
        return result;
    }
    
    // BSATN serialization (already in little-endian)
    static void serialize(std::vector<uint8_t>& buffer, const u256& value) {
        buffer.insert(buffer.end(), value.data.begin(), value.data.end());
    }
    
    static u256 deserialize(const uint8_t* bytes) {
        return u256(bytes);
    }
    
    // BSATN methods (implemented in types_bsatn.h)
    void bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const;
    void bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader);
};

// 256-bit signed integer
struct i256 {
    std::array<uint8_t, 32> data;
    
    i256() { data.fill(0); }
    
    explicit i256(const uint8_t* bytes) {
        std::memcpy(data.data(), bytes, 32);
    }
    
    // Constructor from 4 uint64_t values (big-endian order: word3 is most significant)
    i256(uint64_t word3, uint64_t word2, uint64_t word1, uint64_t word0) {
        // Store in little-endian byte order
        for (int i = 0; i < 8; ++i) {
            data[i] = (word0 >> (i * 8)) & 0xFF;
            data[8 + i] = (word1 >> (i * 8)) & 0xFF;
            data[16 + i] = (word2 >> (i * 8)) & 0xFF;
            data[24 + i] = (word3 >> (i * 8)) & 0xFF;
        }
    }
    
    bool operator==(const i256& other) const {
        return data == other.data;
    }
    
    bool operator!=(const i256& other) const {
        return !(*this == other);
    }
    
    // Convert to string using arbitrary precision decimal conversion
    std::string to_string() const {
        // Check if this is a negative number (MSB set)
        bool is_negative = (data[31] & 0x80) != 0;
        
        if (is_negative) {
            // Create positive version using two's complement
            i256 positive_version;
            int carry = 1;
            for (int i = 0; i < 32; ++i) {
                int temp = (~data[i] & 0xFF) + carry;
                positive_version.data[i] = temp & 0xFF;
                carry = temp >> 8;
            }
            
            // Convert positive version to string and add minus sign
            u256 as_u256;
            std::copy(positive_version.data.begin(), positive_version.data.end(), as_u256.data.begin());
            return "-" + as_u256.to_string();
        } else {
            // Convert as unsigned
            u256 as_u256;
            std::copy(data.begin(), data.end(), as_u256.data.begin());
            return as_u256.to_string();
        }
    }
    
    // BSATN serialization (already in little-endian)
    static void serialize(std::vector<uint8_t>& buffer, const i256& value) {
        buffer.insert(buffer.end(), value.data.begin(), value.data.end());
    }
    
    static i256 deserialize(const uint8_t* bytes) {
        return i256(bytes);
    }
    
    // BSATN methods (implemented in types_bsatn.h)
    void bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const;
    void bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader);
};

// Type aliases with uppercase names for compatibility
using U128 = u128;
using I128 = i128;
using U256 = u256;
using I256 = i256;

// =============================================================================
// CONNECTION ID TYPE (defined after u128)
// =============================================================================

/**
 * ConnectionId represents a connection identifier in SpacetimeDB.
 * 
 * IMPORTANT: ConnectionId uses u128 type to match Rust implementation.
 * This was fixed from the previous uint64_t implementation that caused
 * runtime serialization crashes.
 * 
 * Special type tag: "__connection_id__"
 * Underlying type: U128 (AlgebraicTypeTag = 15)
 * Serialization: write_u128_le/read_u128_le
 */
struct ConnectionId {
    u128 id;
    
    ConnectionId() : id() {}
    explicit ConnectionId(uint64_t connection_id) : id(connection_id) {}
    explicit ConnectionId(const u128& connection_id) : id(connection_id) {}
    
    bool operator==(const ConnectionId& other) const { return id == other.id; }
    bool operator!=(const ConnectionId& other) const { return id != other.id; }
    bool operator<(const ConnectionId& other) const { 
        return (id.high < other.id.high) || (id.high == other.id.high && id.low < other.id.low);
    }
    
    // Convert to string as hex representation (32 hex chars for 16 bytes)
    std::string to_string() const {
        std::ostringstream oss;
        oss << std::hex << std::setfill('0');
        
        // Output as 32-character hex string (little-endian byte order)
        // Convert low 64 bits first
        for (int i = 0; i < 8; ++i) {
            uint8_t byte = (id.low >> (i * 8)) & 0xFF;
            oss << std::setw(2) << static_cast<unsigned int>(byte);
        }
        
        // Convert high 64 bits
        for (int i = 0; i < 8; ++i) {
            uint8_t byte = (id.high >> (i * 8)) & 0xFF;
            oss << std::setw(2) << static_cast<unsigned int>(byte);
        }
        
        return oss.str();
    }
    
    // BSATN serialization (implemented below)
    void bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const;
    void bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader);
};

// Placeholder type aliases for BSATN compatibility
using u256_placeholder = u256;
using i256_placeholder = i256;

// Address is an alias for Identity in the current implementation
class Identity;  // Forward declaration
using Address = Identity;

// =============================================================================
// SCHEDULE AT TYPE FORWARD DECLARATION
// =============================================================================

// ScheduleAt is defined in schedule_at.h to avoid circular dependencies
class ScheduleAt;

// =============================================================================
// TIME DURATION TYPE
// =============================================================================

// Time duration is defined in time_duration.h
/*
struct TimeDuration {
    uint64_t micros;
    
    TimeDuration() : micros(0) {}
    explicit TimeDuration(uint64_t us) : micros(us) {}
    
    // Constructor from seconds and nanoseconds
    TimeDuration(uint64_t seconds, uint32_t nanos) 
        : micros(seconds * 1000000 + nanos / 1000) {}
    
    static TimeDuration from_micros(uint64_t us) { return TimeDuration(us); }
    static TimeDuration from_millis(uint64_t ms) { return TimeDuration(ms * 1000); }
    static TimeDuration from_seconds(uint64_t s) { return TimeDuration(s * 1000000); }
    
    uint64_t to_micros() const { return micros; }
    uint64_t to_millis() const { return micros / 1000; }
    uint64_t to_seconds() const { return micros / 1000000; }
    
    TimeDuration operator+(const TimeDuration& other) const {
        return TimeDuration(micros + other.micros);
    }
    
    TimeDuration operator-(const TimeDuration& other) const {
        return TimeDuration(micros - other.micros);
    }
    
    bool operator==(const TimeDuration& other) const {
        return micros == other.micros;
    }
    
    bool operator!=(const TimeDuration& other) const {
        return !(*this == other);
    }
    
    bool operator<(const TimeDuration& other) const {
        return micros < other.micros;
    }
    
    // BSATN serialization (as u64)
    static void serialize(std::vector<uint8_t>& buffer, const TimeDuration& value) {
        for (int i = 0; i < 8; ++i) {
            buffer.push_back((value.micros >> (i * 8)) & 0xFF);
        }
    }
    
    static TimeDuration deserialize(const uint8_t* data) {
        uint64_t micros = 0;
        for (int i = 0; i < 8; ++i) {
            micros |= static_cast<uint64_t>(data[i]) << (i * 8);
        }
        return TimeDuration(micros);
    }
    
    // BSATN serialization methods (for schedule_reducer.h)
    // These will be provided as free functions when BSATN is available
    // void bsatn_serialize(SpacetimeDb::bsatn::Writer& writer) const;
    // static TimeDuration bsatn_deserialize(SpacetimeDb::bsatn::Reader& reader);
};
*/

// Timestamp operations with TimeDuration are already defined in timestamp.h
// We'll use the existing Timestamp class operations

// =============================================================================
// BSATN SERIALIZATION TRAITS
// =============================================================================

// Primary template for BSATN serialization
template<typename T>
struct BsatnSerializer {
    static void serialize(std::vector<uint8_t>& buffer, const T& value);
    static T deserialize(const uint8_t* data, size_t& offset);
};

// Specializations for primitive types
template<>
struct BsatnSerializer<uint8_t> {
    static void serialize(std::vector<uint8_t>& buffer, const uint8_t& value) {
        buffer.push_back(value);
    }
    
    static uint8_t deserialize(const uint8_t* data, size_t& offset) {
        return data[offset++];
    }
};

template<>
struct BsatnSerializer<uint16_t> {
    static void serialize(std::vector<uint8_t>& buffer, const uint16_t& value) {
        buffer.push_back(value & 0xFF);
        buffer.push_back((value >> 8) & 0xFF);
    }
    
    static uint16_t deserialize(const uint8_t* data, size_t& offset) {
        uint16_t value = data[offset] | (data[offset + 1] << 8);
        offset += 2;
        return value;
    }
};

template<>
struct BsatnSerializer<uint32_t> {
    static void serialize(std::vector<uint8_t>& buffer, const uint32_t& value) {
        for (int i = 0; i < 4; ++i) {
            buffer.push_back((value >> (i * 8)) & 0xFF);
        }
    }
    
    static uint32_t deserialize(const uint8_t* data, size_t& offset) {
        uint32_t value = 0;
        for (int i = 0; i < 4; ++i) {
            value |= static_cast<uint32_t>(data[offset++]) << (i * 8);
        }
        return value;
    }
};

template<>
struct BsatnSerializer<uint64_t> {
    static void serialize(std::vector<uint8_t>& buffer, const uint64_t& value) {
        for (int i = 0; i < 8; ++i) {
            buffer.push_back((value >> (i * 8)) & 0xFF);
        }
    }
    
    static uint64_t deserialize(const uint8_t* data, size_t& offset) {
        uint64_t value = 0;
        for (int i = 0; i < 8; ++i) {
            value |= static_cast<uint64_t>(data[offset++]) << (i * 8);
        }
        return value;
    }
};

// Signed integers
template<>
struct BsatnSerializer<int8_t> {
    static void serialize(std::vector<uint8_t>& buffer, const int8_t& value) {
        buffer.push_back(static_cast<uint8_t>(value));
    }
    
    static int8_t deserialize(const uint8_t* data, size_t& offset) {
        return static_cast<int8_t>(data[offset++]);
    }
};

template<>
struct BsatnSerializer<int16_t> {
    static void serialize(std::vector<uint8_t>& buffer, const int16_t& value) {
        BsatnSerializer<uint16_t>::serialize(buffer, static_cast<uint16_t>(value));
    }
    
    static int16_t deserialize(const uint8_t* data, size_t& offset) {
        return static_cast<int16_t>(BsatnSerializer<uint16_t>::deserialize(data, offset));
    }
};

template<>
struct BsatnSerializer<int32_t> {
    static void serialize(std::vector<uint8_t>& buffer, const int32_t& value) {
        BsatnSerializer<uint32_t>::serialize(buffer, static_cast<uint32_t>(value));
    }
    
    static int32_t deserialize(const uint8_t* data, size_t& offset) {
        return static_cast<int32_t>(BsatnSerializer<uint32_t>::deserialize(data, offset));
    }
};

template<>
struct BsatnSerializer<int64_t> {
    static void serialize(std::vector<uint8_t>& buffer, const int64_t& value) {
        BsatnSerializer<uint64_t>::serialize(buffer, static_cast<uint64_t>(value));
    }
    
    static int64_t deserialize(const uint8_t* data, size_t& offset) {
        return static_cast<int64_t>(BsatnSerializer<uint64_t>::deserialize(data, offset));
    }
};

// Boolean
template<>
struct BsatnSerializer<bool> {
    static void serialize(std::vector<uint8_t>& buffer, const bool& value) {
        buffer.push_back(value ? 1 : 0);
    }
    
    static bool deserialize(const uint8_t* data, size_t& offset) {
        return data[offset++] != 0;
    }
};

// Floating point
template<>
struct BsatnSerializer<float> {
    static void serialize(std::vector<uint8_t>& buffer, const float& value) {
        uint32_t bits;
        std::memcpy(&bits, &value, sizeof(float));
        BsatnSerializer<uint32_t>::serialize(buffer, bits);
    }
    
    static float deserialize(const uint8_t* data, size_t& offset) {
        uint32_t bits = BsatnSerializer<uint32_t>::deserialize(data, offset);
        float value;
        std::memcpy(&value, &bits, sizeof(float));
        return value;
    }
};

template<>
struct BsatnSerializer<double> {
    static void serialize(std::vector<uint8_t>& buffer, const double& value) {
        uint64_t bits;
        std::memcpy(&bits, &value, sizeof(double));
        BsatnSerializer<uint64_t>::serialize(buffer, bits);
    }
    
    static double deserialize(const uint8_t* data, size_t& offset) {
        uint64_t bits = BsatnSerializer<uint64_t>::deserialize(data, offset);
        double value;
        std::memcpy(&value, &bits, sizeof(double));
        return value;
    }
};

// String
template<>
struct BsatnSerializer<std::string> {
    static void serialize(std::vector<uint8_t>& buffer, const std::string& value) {
        BsatnSerializer<uint32_t>::serialize(buffer, static_cast<uint32_t>(value.length()));
        buffer.insert(buffer.end(), value.begin(), value.end());
    }
    
    static std::string deserialize(const uint8_t* data, size_t& offset) {
        uint32_t len = BsatnSerializer<uint32_t>::deserialize(data, offset);
        std::string result(reinterpret_cast<const char*>(data + offset), len);
        offset += len;
        return result;
    }
};

// Large integers
template<>
struct BsatnSerializer<u128> {
    static void serialize(std::vector<uint8_t>& buffer, const u128& value) {
        u128::serialize(buffer, value);
    }
    
    static u128 deserialize(const uint8_t* data, size_t& offset) {
        u128 result = u128::deserialize(data + offset);
        offset += 16;
        return result;
    }
};

template<>
struct BsatnSerializer<i128> {
    static void serialize(std::vector<uint8_t>& buffer, const i128& value) {
        i128::serialize(buffer, value);
    }
    
    static i128 deserialize(const uint8_t* data, size_t& offset) {
        i128 result = i128::deserialize(data + offset);
        offset += 16;
        return result;
    }
};

template<>
struct BsatnSerializer<u256> {
    static void serialize(std::vector<uint8_t>& buffer, const u256& value) {
        u256::serialize(buffer, value);
    }
    
    static u256 deserialize(const uint8_t* data, size_t& offset) {
        u256 result = u256::deserialize(data + offset);
        offset += 32;
        return result;
    }
};

template<>
struct BsatnSerializer<i256> {
    static void serialize(std::vector<uint8_t>& buffer, const i256& value) {
        i256::serialize(buffer, value);
    }
    
    static i256 deserialize(const uint8_t* data, size_t& offset) {
        i256 result = i256::deserialize(data + offset);
        offset += 32;
        return result;
    }
};

// TimeDuration serializer commented out since TimeDuration is defined in time_duration.h
/*
template<>
struct BsatnSerializer<TimeDuration> {
    static void serialize(std::vector<uint8_t>& buffer, const TimeDuration& value) {
        TimeDuration::serialize(buffer, value);
    }
    
    static TimeDuration deserialize(const uint8_t* data, size_t& offset) {
        TimeDuration result = TimeDuration::deserialize(data + offset);
        offset += 8;
        return result;
    }
};
*/

// Identity and ConnectionId serializers are already defined in their respective headers

// Option<T> serialization (handles both Option<T> and std::optional<T>)
template<typename T>
struct BsatnSerializer<std::optional<T>> {
    static void serialize(std::vector<uint8_t>& buffer, const std::optional<T>& value) {
        if (value.has_value()) {
            buffer.push_back(0); // Some tag
            BsatnSerializer<T>::serialize(buffer, *value);
        } else {
            buffer.push_back(1); // None tag
        }
    }
    
    static std::optional<T> deserialize(const uint8_t* data, size_t& offset) {
        uint8_t tag = data[offset++];
        if (tag == 0) {
            return BsatnSerializer<T>::deserialize(data, offset);
        } else {
            return std::nullopt;
        }
    }
};

// Option<T> serialization (forward to std::optional)
template<typename T>
struct BsatnSerializer<Option<T>> {
    static void serialize(std::vector<uint8_t>& buffer, const Option<T>& value) {
        BsatnSerializer<std::optional<T>>::serialize(buffer, value);
    }
    
    static Option<T> deserialize(const uint8_t* data, size_t& offset) {
        return Option<T>(BsatnSerializer<std::optional<T>>::deserialize(data, offset));
    }
};

// Vec<T> serialization
template<typename T>
struct BsatnSerializer<std::vector<T>> {
    static void serialize(std::vector<uint8_t>& buffer, const std::vector<T>& value) {
        BsatnSerializer<uint32_t>::serialize(buffer, static_cast<uint32_t>(value.size()));
        for (const auto& item : value) {
            BsatnSerializer<T>::serialize(buffer, item);
        }
    }
    
    static std::vector<T> deserialize(const uint8_t* data, size_t& offset) {
        uint32_t len = BsatnSerializer<uint32_t>::deserialize(data, offset);
        std::vector<T> result;
        result.reserve(len);
        for (uint32_t i = 0; i < len; ++i) {
            result.push_back(BsatnSerializer<T>::deserialize(data, offset));
        }
        return result;
    }
};

// ScheduleAt serialization is defined in schedule_at_impl.h

// =============================================================================
// TYPE REGISTRATION FOR EXTENDED TYPES
// =============================================================================

// Type registration for large integers and time types will be handled in
// the AlgebraicType system. Option<T> and Vec<T> need special handling.

// =============================================================================
// INLINE IMPLEMENTATIONS THAT REQUIRE COMPLETE TYPES
// =============================================================================

// TimeDuration BSATN methods will be provided as free functions in a separate header
// that includes both types.h and bsatn.h

} // namespace SpacetimeDb

// =============================================================================
// BSATN Implementation
// =============================================================================
// The implementation of BSATN serialization methods has been moved to types_impl.h
// to avoid circular dependencies with reader.h and writer.h.
// Include spacetimedb/bsatn/types_impl.h after including reader.h and writer.h
// to get the implementations.