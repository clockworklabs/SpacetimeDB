#ifndef SPACETIMEDB_LIBRARY_LOGGING_H
#define SPACETIMEDB_LIBRARY_LOGGING_H

#include "spacetimedb/abi/FFI.h" // For LogLevel
#include <string>
#include <string_view>
#include <chrono>
#include <memory>
#include <cstring>
#include <cstdio>

namespace SpacetimeDB {

// LogLevel is now from opaque_types.h, imported via FFI.h

// Compile-time filename extraction to avoid runtime filesystem operations
namespace detail {
    constexpr const char* extract_filename(const char* path) {
        const char* file = path;
        while (*path) {
            if (*path == '/' || *path == '\\') {
                file = path + 1;
            }
            ++path;
        }
        return file;
    }
    
    // Simple printf-style formatting
    template<typename... Args>
    std::string format(const char* fmt, Args... args) {
        // Get required size
        int size = std::snprintf(nullptr, 0, fmt, args...);
        if (size <= 0) { return ""; }
        
        // Format string
        std::string result(size + 1, '\0');
        std::snprintf(result.data(), result.size(), fmt, args...);
        result.resize(size); // Remove null terminator from std::string
        return result;
    }
}

// Compile-time macro for extracting just the filename
#define STDB_FILENAME ::SpacetimeDB::detail::extract_filename(__FILE__)

// Default log level - can be overridden at compile time
#ifndef STDB_LOG_LEVEL
    #ifdef NDEBUG
        #define STDB_LOG_LEVEL ::SpacetimeDB::LogLevelValue::INFO
    #else
        #define STDB_LOG_LEVEL ::SpacetimeDB::LogLevelValue::DEBUG
    #endif
#endif

/**
 * @brief Optimized logging with caller information using string_view.
 * @param level The severity level of the message.
 * @param message The message to log.
 * @param target The function/method name.
 * @param filename The source file name (just filename, not full path).
 * @param line_number The source line number.
 * @ingroup sdk_runtime sdk_logging
 */
inline void log_with_caller_info(LogLevel level, std::string_view message, 
                                std::string_view target = "", 
                                std::string_view filename = "", 
                                uint32_t line_number = 0) {
    FFI::console_log(level,
                   reinterpret_cast<const uint8_t*>(target.data()), target.length(),
                   reinterpret_cast<const uint8_t*>(filename.data()), filename.length(),
                   line_number,
                   reinterpret_cast<const uint8_t*>(message.data()), message.length());
}

/**
 * @brief Simple logging without caller information (optimized).
 * @param level The severity level of the message.
 * @param message The message to log.
 * @ingroup sdk_runtime sdk_logging
 */
inline void log(LogLevel level, std::string_view message) {
    log_with_caller_info(level, message, "", "", 0);
}

// Legacy overloads for backward compatibility
inline void log_with_caller_info(LogLevel level, const std::string& message, 
                                const char* target = nullptr, 
                                const char* filename = nullptr, 
                                uint32_t line_number = 0) {
    log_with_caller_info(level, std::string_view(message), 
                        target ? std::string_view(target) : std::string_view(""),
                        filename ? std::string_view(filename) : std::string_view(""),
                        line_number);
}

inline void log(LogLevel level, const std::string& message) {
    log(level, std::string_view(message));
}

// Optimized logging macros with compile-time level filtering
#define LOG_ERROR(message) \
    do { \
        if constexpr (STDB_LOG_LEVEL >= ::SpacetimeDB::LogLevelValue::ERROR) { \
            ::SpacetimeDB::log_with_caller_info(::SpacetimeDB::LogLevelValue::ERROR, (message), __func__, STDB_FILENAME, __LINE__); \
        } \
    } while(0)

#define LOG_WARN(message) \
    do { \
        if constexpr (STDB_LOG_LEVEL >= ::SpacetimeDB::LogLevelValue::WARN) { \
            ::SpacetimeDB::log_with_caller_info(::SpacetimeDB::LogLevelValue::WARN, (message), __func__, STDB_FILENAME, __LINE__); \
        } \
    } while(0)

#define LOG_INFO(message) \
    do { \
        if constexpr (STDB_LOG_LEVEL >= ::SpacetimeDB::LogLevelValue::INFO) { \
            ::SpacetimeDB::log_with_caller_info(::SpacetimeDB::LogLevelValue::INFO, (message), __func__, STDB_FILENAME, __LINE__); \
        } \
    } while(0)

#define LOG_DEBUG(message) \
    do { \
        if constexpr (STDB_LOG_LEVEL >= ::SpacetimeDB::LogLevelValue::DEBUG) { \
            ::SpacetimeDB::log_with_caller_info(::SpacetimeDB::LogLevelValue::DEBUG, (message), __func__, STDB_FILENAME, __LINE__); \
        } \
    } while(0)

#define LOG_TRACE(message) \
    do { \
        if constexpr (STDB_LOG_LEVEL >= ::SpacetimeDB::LogLevelValue::TRACE) { \
            ::SpacetimeDB::log_with_caller_info(::SpacetimeDB::LogLevelValue::TRACE, (message), __func__, STDB_FILENAME, __LINE__); \
        } \
    } while(0)

// Printf-style logging macros for convenience
#define LOG_ERROR_F(fmt, ...) LOG_ERROR(::SpacetimeDB::detail::format(fmt, ##__VA_ARGS__))
#define LOG_WARN_F(fmt, ...) LOG_WARN(::SpacetimeDB::detail::format(fmt, ##__VA_ARGS__))
#define LOG_INFO_F(fmt, ...) LOG_INFO(::SpacetimeDB::detail::format(fmt, ##__VA_ARGS__))
#define LOG_DEBUG_F(fmt, ...) LOG_DEBUG(::SpacetimeDB::detail::format(fmt, ##__VA_ARGS__))
#define LOG_TRACE_F(fmt, ...) LOG_TRACE(::SpacetimeDB::detail::format(fmt, ##__VA_ARGS__))

// Special panic macro that logs as ERROR level matching to LOG_FATAL
// ERROR level is used due to PANIC logging a stack trace which won't work correctly from C++
#define LOG_PANIC(message) \
    do { \
        ::SpacetimeDB::log_with_caller_info(::SpacetimeDB::LogLevelValue::ERROR, (message), __func__, STDB_FILENAME, __LINE__); \
        __builtin_trap(); \
    } while(0)

// Fatal error macro that logs and aborts (for exception-free code)
#define LOG_FATAL(message) \
    do { \
        ::SpacetimeDB::log_with_caller_info(::SpacetimeDB::LogLevelValue::ERROR, (message), __func__, STDB_FILENAME, __LINE__); \
        __builtin_trap(); \
    } while(0)

// Convenience functions using string_view (inline for optimization)
inline void log_error(std::string_view message) { log(LogLevelValue::ERROR, message); }
inline void log_warn(std::string_view message) { log(LogLevelValue::WARN, message); }
inline void log_info(std::string_view message) { log(LogLevelValue::INFO, message); }
inline void log_debug(std::string_view message) { log(LogLevelValue::DEBUG, message); }
inline void log_trace(std::string_view message) { log(LogLevelValue::TRACE, message); }
inline void log_panic(std::string_view message) { log(LogLevelValue::ERROR, message); } // Panic logs as PANIC

// Legacy overloads for backward compatibility
inline void log_error(const std::string& message) { log_error(std::string_view(message)); }
inline void log_warn(const std::string& message) { log_warn(std::string_view(message)); }
inline void log_info(const std::string& message) { log_info(std::string_view(message)); }
inline void log_debug(const std::string& message) { log_debug(std::string_view(message)); }
inline void log_trace(const std::string& message) { log_trace(std::string_view(message)); }

/**
 * @brief Optimized RAII performance measurement utility.
 * 
 * This class provides automatic performance timing with SpacetimeDB's
 * console timer system. The timer starts when constructed and automatically
 * ends when the object is destroyed (RAII pattern).
 * 
 * Improvements:
 * - Uses string_view to avoid allocations
 * - Supports retrieving elapsed time
 * - Better move semantics
 * - Inline implementation for better optimization
 * 
 * Example usage:
 * @code
 * {
 *     LogStopwatch timer("database_operation");
 *     // ... perform database operations ...
 *     // Timer automatically ends when timer goes out of scope
 * }
 * @endcode
 * 
 * @ingroup sdk_runtime sdk_logging
 */
class LogStopwatch {
public:
    /**
     * @brief Start a performance timer with the given name.
     * @param name The name of the operation being timed.
     */
    explicit LogStopwatch(std::string_view name) : ended_(false) {
        timer_id_ = FFI::console_timer_start(
            reinterpret_cast<const uint8_t*>(name.data()),
            name.length()
        );
        start_time_ = std::chrono::steady_clock::now();
    }
    
    // Constructor accepting std::string for backward compatibility
    explicit LogStopwatch(const std::string& name) : LogStopwatch(std::string_view(name)) {}
    
    /**
     * @brief Destructor automatically ends the timer.
     */
    ~LogStopwatch() {
        if (!ended_) {
            end();
        }
    }
    
    /**
     * @brief Manually end the timer (optional - destructor will do this automatically).
     */
    void end() {
        if (!ended_) {
            auto status = FFI::console_timer_end(timer_id_);
            ended_ = true;
            end_time_ = std::chrono::steady_clock::now();
            // TODO: Add error handling when we implement exception system
            (void)status; // Suppress unused variable warning for now
        }
    }
    
    /**
     * @brief Get elapsed time in microseconds.
     * @return Elapsed time or 0 if timer hasn't ended.
     */
    uint64_t elapsed_microseconds() const {
        if (ended_) {
            return std::chrono::duration_cast<std::chrono::microseconds>(
                end_time_ - start_time_).count();
        }
        // Return current elapsed time if not ended
        auto now = std::chrono::steady_clock::now();
        return std::chrono::duration_cast<std::chrono::microseconds>(
            now - start_time_).count();
    }
    
    /**
     * @brief Get elapsed time in milliseconds.
     * @return Elapsed time or 0 if timer hasn't ended.
     */
    uint64_t elapsed_milliseconds() const {
        return elapsed_microseconds() / 1000;
    }
    
    // Disable copy construction and assignment
    LogStopwatch(const LogStopwatch&) = delete;
    LogStopwatch& operator=(const LogStopwatch&) = delete;
    
    // Allow move construction and assignment
    LogStopwatch(LogStopwatch&& other) noexcept
        : timer_id_(other.timer_id_), 
          ended_(other.ended_),
          start_time_(other.start_time_),
          end_time_(other.end_time_) {
        other.ended_ = true; // Prevent double-ending
    }
    
    LogStopwatch& operator=(LogStopwatch&& other) noexcept {
        if (this != &other) {
            if (!ended_) {
                end(); // End current timer
            }
            timer_id_ = other.timer_id_;
            ended_ = other.ended_;
            start_time_ = other.start_time_;
            end_time_ = other.end_time_;
            other.ended_ = true; // Prevent double-ending
        }
        return *this;
    }

private:
    ConsoleTimerId timer_id_;
    bool ended_;
    std::chrono::steady_clock::time_point start_time_;
    std::chrono::steady_clock::time_point end_time_;
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_LIBRARY_LOGGING_H
