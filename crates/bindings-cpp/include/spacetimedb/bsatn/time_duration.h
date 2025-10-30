#pragma once

#include <cstdint>
#include <chrono>

// Forward declarations for BSATN
namespace SpacetimeDb {
namespace bsatn {
    class Writer;
    class Reader;
}
}

namespace SpacetimeDb {

/**
 * Represents a duration of time with microsecond precision.
 * This corresponds to SpacetimeDB's TimeDuration type.
 */
class TimeDuration {
private:
    int64_t micros_;

public:
    // Constructors
    explicit TimeDuration(int64_t micros = 0) : micros_(micros) {}
    
    // Factory methods
    static TimeDuration from_micros(int64_t micros) { return TimeDuration(micros); }
    static TimeDuration from_millis(int64_t millis) { return TimeDuration(millis * 1000); }
    static TimeDuration from_seconds(int64_t seconds) { return TimeDuration(seconds * 1000000); }
    static TimeDuration from_minutes(int64_t minutes) { return TimeDuration(minutes * 60000000); }
    static TimeDuration from_hours(int64_t hours) { return TimeDuration(hours * 3600000000); }
    
    // Conversion from std::chrono
    template<typename Rep, typename Period>
    static TimeDuration from_chrono(std::chrono::duration<Rep, Period> duration) {
        auto micros = std::chrono::duration_cast<std::chrono::microseconds>(duration).count();
        return TimeDuration(static_cast<int64_t>(micros));
    }
    
    // Getters
    int64_t micros() const { return micros_; }
    int64_t millis() const { return micros_ / 1000; }
    int64_t seconds() const { return micros_ / 1000000; }
    
    // Conversion to std::chrono
    std::chrono::microseconds to_chrono() const {
        return std::chrono::microseconds(micros_);
    }
    
    // Arithmetic operations
    TimeDuration operator+(const TimeDuration& other) const {
        return TimeDuration(micros_ + other.micros_);
    }
    
    TimeDuration operator-(const TimeDuration& other) const {
        return TimeDuration(micros_ - other.micros_);
    }
    
    TimeDuration operator*(int64_t scalar) const {
        return TimeDuration(micros_ * scalar);
    }
    
    TimeDuration operator/(int64_t scalar) const {
        return TimeDuration(micros_ / scalar);
    }
    
    // Comparison operators
    bool operator==(const TimeDuration& other) const { return micros_ == other.micros_; }
    bool operator!=(const TimeDuration& other) const { return micros_ != other.micros_; }
    bool operator<(const TimeDuration& other) const { return micros_ < other.micros_; }
    bool operator<=(const TimeDuration& other) const { return micros_ <= other.micros_; }
    bool operator>(const TimeDuration& other) const { return micros_ > other.micros_; }
    bool operator>=(const TimeDuration& other) const { return micros_ >= other.micros_; }
    
    // Absolute value
    TimeDuration abs() const {
        return TimeDuration(micros_ >= 0 ? micros_ : -micros_);
    }
    
    // Convert to string representation (to match Rust TimeDuration::Display format exactly)
    std::string to_string() const {
        // Rust format: "{sign}{seconds}.{microseconds:06}"
        // Always includes sign prefix: "+" for positive, "-" for negative
        
        int64_t micros = micros_;
        const char* sign = (micros < 0) ? "-" : "+";
        int64_t pos_micros = (micros < 0) ? -micros : micros;
        
        int64_t seconds = pos_micros / 1000000;
        int64_t remaining_micros = pos_micros % 1000000;
        
        char buffer[32];
        snprintf(buffer, sizeof(buffer), "%s%lld.%06lld", 
                 sign, (long long)seconds, (long long)remaining_micros);
        
        return std::string(buffer);
    }
    
    // BSATN serialization (implemented in time_duration_bsatn.h)
    void bsatn_serialize(SpacetimeDb::bsatn::Writer& writer) const;
    static TimeDuration bsatn_deserialize(SpacetimeDb::bsatn::Reader& reader);
};

// Convenience functions
inline TimeDuration operator*(int64_t scalar, const TimeDuration& duration) {
    return duration * scalar;
}

} // namespace SpacetimeDb

// =============================================================================
// BSATN Implementation
// =============================================================================

#include "writer.h"
#include "reader.h"

namespace SpacetimeDb {

// TimeDuration BSATN implementation
inline void TimeDuration::bsatn_serialize(bsatn::Writer& writer) const {
    writer.write_u64_le(static_cast<uint64_t>(micros_));
}

inline TimeDuration TimeDuration::bsatn_deserialize(bsatn::Reader& reader) {
    uint64_t micros = reader.read_u64_le();
    return TimeDuration(static_cast<int64_t>(micros));
}

} // namespace SpacetimeDb


