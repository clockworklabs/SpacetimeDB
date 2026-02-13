#pragma once

#include <cstdint>
#include <chrono>
#include <ctime>
#include <string>
#include "time_duration.h"

// Forward declarations
namespace SpacetimeDb {
namespace bsatn {
    class Writer;
    class Reader;
}

/**
 * Represents a point in time as microseconds since the Unix epoch.
 * This corresponds to SpacetimeDB's Timestamp type.
 */
class Timestamp {
private:
    int64_t micros_since_epoch_;

public:
    // Constructors
    explicit Timestamp(int64_t micros_since_epoch = 0) : micros_since_epoch_(micros_since_epoch) {}
    
    // Factory methods
    static Timestamp from_micros_since_epoch(int64_t micros) {
        return Timestamp(micros);
    }
    
    static Timestamp from_millis_since_epoch(int64_t millis) {
        return Timestamp(millis * 1000);
    }
    
    static Timestamp from_seconds_since_epoch(int64_t seconds) {
        return Timestamp(seconds * 1000000);
    }
    
    // Get current timestamp
    static Timestamp now() {
        auto now = std::chrono::system_clock::now();
        auto duration = now.time_since_epoch();
        auto micros = std::chrono::duration_cast<std::chrono::microseconds>(duration).count();
        return Timestamp(micros);
    }
    
    // Unix epoch (January 1, 1970 00:00:00 UTC)
    static Timestamp unix_epoch() {
        return Timestamp(0);
    }
    
    // Conversion from std::chrono
    static Timestamp from_chrono(std::chrono::system_clock::time_point tp) {
        auto duration = tp.time_since_epoch();
        auto micros = std::chrono::duration_cast<std::chrono::microseconds>(duration).count();
        return Timestamp(micros);
    }
    
    // Getters
    int64_t micros_since_epoch() const { return micros_since_epoch_; }
    int64_t millis_since_epoch() const { return micros_since_epoch_ / 1000; }
    int64_t seconds_since_epoch() const { return micros_since_epoch_ / 1000000; }
    
    // Conversion to std::chrono
    std::chrono::system_clock::time_point to_chrono() const {
        return std::chrono::system_clock::time_point(
            std::chrono::microseconds(micros_since_epoch_)
        );
    }
    
    // Arithmetic operations
    Timestamp operator+(const TimeDuration& duration) const {
        return Timestamp(micros_since_epoch_ + duration.micros());
    }
    
    Timestamp operator-(const TimeDuration& duration) const {
        return Timestamp(micros_since_epoch_ - duration.micros());
    }
    
    TimeDuration operator-(const Timestamp& other) const {
        return TimeDuration::from_micros(micros_since_epoch_ - other.micros_since_epoch_);
    }
    
    // Comparison operators
    bool operator==(const Timestamp& other) const { return micros_since_epoch_ == other.micros_since_epoch_; }
    bool operator!=(const Timestamp& other) const { return micros_since_epoch_ != other.micros_since_epoch_; }
    bool operator<(const Timestamp& other) const { return micros_since_epoch_ < other.micros_since_epoch_; }
    bool operator<=(const Timestamp& other) const { return micros_since_epoch_ <= other.micros_since_epoch_; }
    bool operator>(const Timestamp& other) const { return micros_since_epoch_ > other.micros_since_epoch_; }
    bool operator>=(const Timestamp& other) const { return micros_since_epoch_ >= other.micros_since_epoch_; }
    
    // Duration since another timestamp
    TimeDuration duration_since(const Timestamp& earlier) const {
        if (*this < earlier) {
            return TimeDuration::from_micros(0);  // Return zero for negative durations
        }
        return TimeDuration::from_micros(micros_since_epoch_ - earlier.micros_since_epoch_);
    }
    
    // Convert to string representation (ISO 8601 format to match Rust)
    std::string to_string() const {
        // Convert microseconds to seconds and fractional microseconds
        std::time_t seconds = micros_since_epoch_ / 1000000;
        int64_t remaining_micros = micros_since_epoch_ % 1000000;
        
        // Handle negative timestamps
        if (micros_since_epoch_ < 0 && remaining_micros != 0) {
            seconds -= 1;
            remaining_micros = 1000000 + remaining_micros;
        }
        
        // Convert to UTC time
#if defined(_MSC_VER) && !defined(__EMSCRIPTEN__)
        // Use gmtime_s on Windows (not available in Emscripten)
        std::tm utc_time_buf;
        errno_t err = gmtime_s(&utc_time_buf, &seconds);
        std::tm* utc_time = (err == 0) ? &utc_time_buf : nullptr;
#else
        // Use standard gmtime for WASM/Emscripten and other platforms
        std::tm* utc_time = std::gmtime(&seconds);
#endif
        if (!utc_time) {
            // Fallback for invalid timestamps
            return "1970-01-01T00:00:00.000000+00:00";
        }
        
        // Format as ISO 8601 with microseconds
        char buffer[64];
        std::strftime(buffer, sizeof(buffer), "%Y-%m-%dT%H:%M:%S", utc_time);
        
        // Add microseconds and timezone
        std::string result(buffer);
        char micros_buffer[16];
        snprintf(micros_buffer, sizeof(micros_buffer), ".%06lld+00:00", (long long)remaining_micros);
        result += micros_buffer;
        
        return result;
    }
    
    // BSATN serialization
    void bsatn_serialize(SpacetimeDb::bsatn::Writer& writer) const;
    static Timestamp bsatn_deserialize(SpacetimeDb::bsatn::Reader& reader);
};

// Convenience operators
inline Timestamp operator+(const TimeDuration& duration, const Timestamp& timestamp) {
    return timestamp + duration;
}

} // namespace SpacetimeDb

// =============================================================================
// BSATN Implementation
// =============================================================================

#include "writer.h"
#include "reader.h"
#include "traits.h"
#include "algebraic_type.h"

namespace SpacetimeDb {

// Timestamp BSATN implementation
inline void Timestamp::bsatn_serialize(SpacetimeDb::bsatn::Writer& writer) const {
    writer.write_i64_le(micros_since_epoch_);
}

inline Timestamp Timestamp::bsatn_deserialize(SpacetimeDb::bsatn::Reader& reader) {
    int64_t micros = reader.read_i64_le();
    return Timestamp(micros);
}

} // namespace SpacetimeDb

// Note: bsatn_traits specialization for Timestamp is defined in type_extensions.h
// to ensure consistent handling with other special types like TimeDuration