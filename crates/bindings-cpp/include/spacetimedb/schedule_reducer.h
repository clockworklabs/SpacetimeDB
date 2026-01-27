#ifndef SPACETIMEDB_SCHEDULE_REDUCER_H
#define SPACETIMEDB_SCHEDULE_REDUCER_H

// ============================================================================
// DEPRECATED: This file contains unused dead code
// ============================================================================
// This file documented a runtime registration API for scheduled reducers that
// was never implemented. The actual C++ scheduled table API uses:
// - SPACETIMEDB_SCHEDULE(table_name, column_index, reducer_name) macro
// - ScheduleAt type for scheduled_at columns
// - See table_with_constraints.h for the real implementation
// - See modules/module-test-cpp/src/lib.cpp for working examples
//
// All code below is commented out pending deletion.
// ============================================================================

/*
#include "spacetimedb/bsatn/types.h"
#include <chrono>
#include <string>

namespace SpacetimeDB {

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
    
    template<typename Rep, typename Period>
    static Duration from_chrono(std::chrono::duration<Rep, Period> duration) {
        auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(duration);
        return Duration(static_cast<uint64_t>(ms.count()));
    }
};

class ScheduleReducer {
public:
    static void register_scheduled([[maybe_unused]] const char* reducer_name, [[maybe_unused]] Duration interval) {
        #ifdef DEBUG
        SpacetimeDB::Log::debug("Scheduling reducer", reducer_name, "with interval", 
                                std::to_string(interval.milliseconds), "ms");
        #endif
    }
    
    static void register_scheduled_at([[maybe_unused]] const char* reducer_name) {
        #ifdef DEBUG
        SpacetimeDB::Log::debug("Registering scheduled_at reducer", reducer_name);
        #endif
    }
    
    static bool validate_cron_expression(const std::string& cron_expr) {
        if (cron_expr.empty()) return false;
        
        int spaces = 0;
        for (char c : cron_expr) {
            if (c == ' ') spaces++;
        }
        
        return spaces == 4;
    }
};

} // namespace SpacetimeDB

namespace SpacetimeDB {
    using Duration = SpacetimeDB::Duration;
    using ScheduleReducer = SpacetimeDB::ScheduleReducer;
}
*/

#endif // SPACETIMEDB_SCHEDULE_REDUCER_H