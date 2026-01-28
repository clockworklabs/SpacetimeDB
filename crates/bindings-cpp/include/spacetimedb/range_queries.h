#pragma once

#include <concepts>
#include <optional>
#include <type_traits>

namespace SpacetimeDB {

// =============================================================================
// C++20 Concepts-based Range Query System
// =============================================================================

/// Concept defining types that can be used in range queries
template<typename T>
concept Rangeable = std::totally_ordered<T> && std::copyable<T> && std::destructible<T>;

/// Range bound types for different query patterns
enum class RangeBound {
    Exclusive,  // 25..30 (excludes end)
    Inclusive   // 25..=30 (includes end) 
};

/// Core Range type supporting all Rust range patterns
template<Rangeable T>
struct Range {
    std::optional<T> start;  // None for ..30
    std::optional<T> end;    // None for 25..
    RangeBound bound_type = RangeBound::Exclusive;
    
    // Constructors for different range types
    Range() = default;
    
    Range(std::optional<T> start_val, std::optional<T> end_val, RangeBound bound = RangeBound::Exclusive)
        : start(start_val), end(end_val), bound_type(bound) {}
    
    // Copy/move constructors
    Range(const Range&) = default;
    Range(Range&&) = default;
    Range& operator=(const Range&) = default;
    Range& operator=(Range&&) = default;
    
    // Check if value is in range
    constexpr bool contains(const T& value) const {
        bool within_start = !start || value >= *start;
        bool within_end;
        
        if (!end) {
            within_end = true;  // No upper bound
        } else {
            within_end = (bound_type == RangeBound::Inclusive) 
                        ? (value <= *end)
                        : (value < *end);
        }
        
        return within_start && within_end;
    }
    
    // Debug string representation
    std::string to_string() const {
        std::string result;
        
        if (start) {
            result += std::to_string(*start);
        }
        
        result += (bound_type == RangeBound::Inclusive) ? "..=" : "..";
        
        if (end) {
            result += std::to_string(*end);
        }
        
        return result;
    }
};

// =============================================================================
// Factory functions for different range patterns (Rust-like syntax)
// =============================================================================

/// Create range from start to end (exclusive): 25..30
template<Rangeable T>
constexpr auto range(T start, T end) -> Range<T> {
    return Range<T>(start, end, RangeBound::Exclusive);
}

/// Create inclusive range from start to end: 25..=30  
template<Rangeable T>
constexpr auto range_inclusive(T start, T end) -> Range<T> {
    return Range<T>(start, end, RangeBound::Inclusive);
}

/// Create range from start with no upper bound: 25..
template<Rangeable T>
constexpr auto range_from(T start) -> Range<T> {
    return Range<T>(start, std::nullopt, RangeBound::Exclusive);
}

/// Create range with no lower bound to end (exclusive): ..30
template<Rangeable T>  
constexpr auto range_to(T end) -> Range<T> {
    return Range<T>(std::nullopt, end, RangeBound::Exclusive);
}

/// Create inclusive range with no lower bound to end: ..=30
template<Rangeable T>
constexpr auto range_to_inclusive(T end) -> Range<T> {
    return Range<T>(std::nullopt, end, RangeBound::Inclusive);
}

/// Create unbounded range (all values): ..
template<Rangeable T>
constexpr auto range_full() -> Range<T> {
    return Range<T>();
}

// =============================================================================
// Type trait to detect Range types
// =============================================================================

template<typename T>
struct is_range : std::false_type {};

template<Rangeable T>
struct is_range<Range<T>> : std::true_type {};

template<typename T>
constexpr bool is_range_v = is_range<T>::value;

// Concept for range types
template<typename T>
concept RangeType = is_range_v<T>;

// =============================================================================
// Integration with field accessors
// =============================================================================

/// Extend field accessor base class with range query support
template<typename Derived, typename FieldType>
requires Rangeable<FieldType>
class RangeQueryAccessor {
public:
    /// Filter by exact value (existing functionality)
    auto filter(const FieldType& value) {
        return static_cast<Derived*>(this)->filter_impl(value);
    }
    
    /// Filter by range (NEW - major feature addition)
    auto filter(const Range<FieldType>& range) {
        return static_cast<Derived*>(this)->filter_range_impl(range);
    }
    
    /// Delete all rows in range (NEW)
    uint32_t delete_range(const Range<FieldType>& range) {
        return static_cast<Derived*>(this)->delete_range_impl(range);
    }
    
    /// Count rows in range (NEW)
    size_t count_range(const Range<FieldType>& range) {
        return static_cast<Derived*>(this)->count_range_impl(range);
    }
};

// =============================================================================
// Convenient operator overloads for natural syntax
// =============================================================================

/// Enable range construction with custom operators
namespace range_operators {
    /// Placeholder type for range construction
    struct RangeStart {};
    struct RangeEnd {};
    
    constexpr RangeStart range_start{};
    constexpr RangeEnd range_end{};
}

} // namespace SpacetimeDB

// =============================================================================
// Usage Examples (for documentation)
// =============================================================================

/*
// Rust patterns                    C++ equivalent
// ---------------                  --------------
// ctx.db.person().age().filter(25..)     → ctx.db[person_age].filter(range_from(25))
// ctx.db.person().age().filter(..30)     → ctx.db[person_age].filter(range_to(30))  
// ctx.db.person().age().filter(25..30)   → ctx.db[person_age].filter(range(25, 30))
// ctx.db.person().age().filter(25..=30)  → ctx.db[person_age].filter(range_inclusive(25, 30))
// ctx.db.person().age().filter(..)       → ctx.db[person_age].filter(range_full<uint8_t>())

Example usage in a reducer:

SPACETIMEDB_REDUCER(test_range_queries, ReducerContext ctx) {
    using namespace SpacetimeDB;
    
    // Find all people aged 25 and above
    auto adults = ctx.db[person_age].filter(range_from(25));
    
    // Find people aged 18-25 (exclusive end) 
    auto young_adults = ctx.db[person_age].filter(range(18, 25));
    
    // Find people up to and including age 65
    auto working_age = ctx.db[person_age].filter(range_to_inclusive(65));
    
    // Delete all people in a specific age range
    uint32_t deleted = ctx.db[person_age].delete_range(range(100, 150));
    
    // Count people in range
    size_t count = ctx.db[person_age].count_range(range_from(50));
}
*/