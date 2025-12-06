#ifndef SPACETIMEDB_BSATN_SCHEDULE_AT_H
#define SPACETIMEDB_BSATN_SCHEDULE_AT_H

#include "timestamp.h"
#include "time_duration.h"
#include <stdexcept>

namespace SpacetimeDb {

/**
 * ScheduleAt represents when a scheduled reducer should execute.
 * This is a sum type with two variants:
 * - Interval(TimeDuration): Execute at regular intervals
 * - Time(Timestamp): Execute at a specific time
 * 
 * Special type matching Rust's ScheduleAt enum.
 * This enables scheduled reducers functionality in SpacetimeDB.
 */
class ScheduleAt {
public:
    enum class Variant : uint8_t {
        Interval = 0,  // Regular interval (TimeDuration)
        Time = 1       // Specific time (Timestamp)
    };
    
private:
    Variant variant;
    union {
        TimeDuration interval_value;
        Timestamp time_value;
    };
    
public:
    // Constructors
    ScheduleAt() : variant(Variant::Interval), interval_value(TimeDuration::from_seconds(0)) {}
    
    ScheduleAt(const TimeDuration& dur) 
        : variant(Variant::Interval), interval_value(dur) {}
    
    ScheduleAt(const Timestamp& ts) 
        : variant(Variant::Time), time_value(ts) {}
    
    // Copy constructor
    ScheduleAt(const ScheduleAt& other) : variant(other.variant) {
        switch (variant) {
            case Variant::Interval:
                new (&interval_value) TimeDuration(other.interval_value);
                break;
            case Variant::Time:
                new (&time_value) Timestamp(other.time_value);
                break;
        }
    }
    
    // Assignment operator
    ScheduleAt& operator=(const ScheduleAt& other) {
        if (this != &other) {
            this->~ScheduleAt(); // Destroy current value
            variant = other.variant;
            switch (variant) {
                case Variant::Interval:
                    new (&interval_value) TimeDuration(other.interval_value);
                    break;
                case Variant::Time:
                    new (&time_value) Timestamp(other.time_value);
                    break;
            }
        }
        return *this;
    }
    
    // Move constructor
    ScheduleAt(ScheduleAt&& other) noexcept : variant(other.variant) {
        switch (variant) {
            case Variant::Interval:
                new (&interval_value) TimeDuration(std::move(other.interval_value));
                break;
            case Variant::Time:
                new (&time_value) Timestamp(std::move(other.time_value));
                break;
        }
    }
    
    // Move assignment
    ScheduleAt& operator=(ScheduleAt&& other) noexcept {
        if (this != &other) {
            this->~ScheduleAt();
            variant = other.variant;
            switch (variant) {
                case Variant::Interval:
                    new (&interval_value) TimeDuration(std::move(other.interval_value));
                    break;
                case Variant::Time:
                    new (&time_value) Timestamp(std::move(other.time_value));
                    break;
            }
        }
        return *this;
    }
    
    // Destructor
    ~ScheduleAt() {
        switch (variant) {
            case Variant::Interval:
                interval_value.~TimeDuration();
                break;
            case Variant::Time:
                time_value.~Timestamp();
                break;
        }
    }
    
    // Accessors
    Variant get_variant() const { return variant; }
    bool is_interval() const { return variant == Variant::Interval; }
    bool is_time() const { return variant == Variant::Time; }
    
    const TimeDuration& get_interval() const {
        if (variant != Variant::Interval) {
            static TimeDuration default_duration;
            return default_duration;
        }
        return interval_value;
    }
    
    const Timestamp& get_time() const {
        if (variant != Variant::Time) {
            static Timestamp default_timestamp;
            return default_timestamp;
        }
        return time_value;
    }
    
    // Static factory methods
    static ScheduleAt interval(const TimeDuration& dur) {
        return ScheduleAt(dur);
    }
    
    static ScheduleAt time(const Timestamp& ts) {
        return ScheduleAt(ts);
    }
    
    // Comparison operators
    bool operator==(const ScheduleAt& other) const {
        if (variant != other.variant) return false;
        switch (variant) {
            case Variant::Interval:
                return interval_value == other.interval_value;
            case Variant::Time:
                return time_value == other.time_value;
        }
        return false;
    }
    
    bool operator!=(const ScheduleAt& other) const {
        return !(*this == other);
    }
    
    // BSATN serialization (implemented below)
    void bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const;
    void bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader);
};

} // namespace SpacetimeDb

#endif // SPACETIMEDB_BSATN_SCHEDULE_AT_H