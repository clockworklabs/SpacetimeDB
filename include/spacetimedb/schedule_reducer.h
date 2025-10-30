#ifndef SPACETIMEDB_SCHEDULE_REDUCER_H
#define SPACETIMEDB_SCHEDULE_REDUCER_H

#include "spacetimedb/bsatn/types.h"
#include <chrono>
#include <string>

namespace SpacetimeDb {

// Duration type that matches SpacetimeDB's expectations
struct Duration {
    uint64_t milliseconds;
    
    Duration(uint64_t ms) : milliseconds(ms) {}
    
    static Duration from_seconds(uint64_t seconds) {
        return Duration(seconds * 1000);
    }
    
    static Duration from_minutes(uint64_t minutes) {
        return Duration(minutes * 60 * 1000);
    }
    
    static Duration from_hours(uint64_t hours) {
        return Duration(hours * 60 * 60 * 1000);
    }
    
    static Duration from_milliseconds(uint64_t ms) {
        return Duration(ms);
    }
    
    // Support conversion from std::chrono types
    template<typename Rep, typename Period>
    static Duration from_chrono(std::chrono::duration<Rep, Period> duration) {
        auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(duration);
        return Duration(static_cast<uint64_t>(ms.count()));
    }
};

// ScheduleReducer class to handle scheduled reducer registration
class ScheduleReducer {
public:
    // Register a reducer to run at fixed intervals
    static void register_scheduled([[maybe_unused]] const char* reducer_name, [[maybe_unused]] Duration interval) {
        // The actual scheduling happens during module description generation
        // This is stored and used when __describe_module__ is called
        // For now, we'll use the existing FFI::schedule_reducer function
        // which takes the reducer ID (we'll use the name for now)
        
        // TODO: This needs to be integrated with the module description system
        // For now, log that we're attempting to schedule
        #ifdef DEBUG
        SpacetimeDb::Log::debug("Scheduling reducer", reducer_name, "with interval", 
                                std::to_string(interval.milliseconds), "ms");
        #endif
    }
    
    // Register a reducer to run at specific times (cron-style)
    static void register_scheduled_at([[maybe_unused]] const char* reducer_name) {
        // This will be called for reducers that have a scheduled_at column
        // The actual scheduling is handled by SpacetimeDB based on table rows
        #ifdef DEBUG
        SpacetimeDb::Log::debug("Registering scheduled_at reducer", reducer_name);
        #endif
    }
    
    // Validate cron expression (placeholder for future implementation)
    static bool validate_cron_expression(const std::string& cron_expr) {
        // TODO: Implement actual cron validation
        // For now, just check it's not empty and has the right format
        // Basic cron format: "minute hour day month weekday"
        if (cron_expr.empty()) return false;
        
        // Count spaces to ensure we have 5 fields
        int spaces = 0;
        for (char c : cron_expr) {
            if (c == ' ') spaces++;
        }
        
        return spaces == 4; // 5 fields = 4 spaces
    }
};

} // namespace SpacetimeDb

// Legacy namespace support
namespace SpacetimeDb {
    using Duration = SpacetimeDb::Duration;
    using ScheduleReducer = SpacetimeDb::ScheduleReducer;
}

#endif // SPACETIMEDB_SCHEDULE_REDUCER_H