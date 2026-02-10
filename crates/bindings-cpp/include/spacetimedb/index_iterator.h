#ifndef SPACETIMEDB_INDEX_ITERATOR_H
#define SPACETIMEDB_INDEX_ITERATOR_H

/**
 * @file index_iterator.h
 * @brief Iterator for traversing indexed fields in SpacetimeDB tables
 * 
 * IndexIterator provides efficient access to rows matching specific values or ranges
 * on indexed fields. Developers use indexed fields through the high-level `filter()` API
 * on field accessors (created by FIELD_Index macro), which internally returns an IndexIterator.
 * 
 * The filter() API provides a clean, intuitive interface for index-based queries without
 * requiring manual index ID management.
 * 
 * @example Basic usage with exact value matching:
 * @code
 * // Declare indexed field
 * FIELD_Index(person, age);
 * 
 * // In a view or reducer, query persons with age 25
 * // The filter() method returns an IndexIteratorRange for clean syntax
 * for (const auto& person : ctx.db[person_age].filter(25u)) {
 *     // Process person aged 25...
 * }
 * @endcode
 * 
 * @example Range queries for filtering within bounds:
 * @code
 * // Query persons between ages 25-30 (inclusive)
 * auto age_range = range_inclusive(uint8_t(25), uint8_t(30));
 * for (const auto& person : ctx.db[person_age].filter(age_range)) {
 *     // Process persons in age range...
 * }
 * 
 * // Query persons 18 and older
 * auto adult_range = range_from(uint8_t(18));
 * for (const auto& person : ctx.db[person_age].filter(adult_range)) {
 *     // Process adult persons...
 * }
 * 
 * // Query persons under 30
 * auto young_range = range_to(uint8_t(30));
 * size_t count = ctx.db[person_age].filter(young_range).size();
 * @endcode
 * 
 * @see range_from, range_to, range_inclusive, range_to_inclusive, range_full for range construction
 * @see FIELD_Index for declaring indexed fields
 * @note IndexIterator is typically used indirectly through ctx.db[field_accessor].filter()
 */

#include "spacetimedb/bsatn/types.h"
#include <spacetimedb/bsatn/bsatn.h>
#include <spacetimedb/abi/FFI.h>
#include <spacetimedb/bsatn/reader.h>
#include <spacetimedb/bsatn/writer.h>
#include <spacetimedb/bsatn/traits.h>
#include <spacetimedb/logger.h>
#include <spacetimedb/range_queries.h>

#include <string>
#include <vector>
#include <tuple>
#include <stdexcept>
#include <optional>
#include <algorithm>
#include <utility>
#include <memory>

namespace SpacetimeDB {

// =============================================================================
// Type traits and tags for query detection
// =============================================================================

/// Tag types for constructor disambiguation
struct exact_match_tag {};
struct prefix_match_tag {};

/// Detect if a type is std::tuple
template<typename T>
struct is_tuple : std::false_type {};

template<typename... Args>
struct is_tuple<std::tuple<Args...>> : std::true_type {};

template<typename T>
inline constexpr bool is_tuple_v = is_tuple<T>::value;

// =============================================================================
// IndexIterator - Efficient index-based iteration
// =============================================================================

template<typename T>
class IndexIterator {
    static_assert(std::is_same_v<T, std::remove_cv_t<T>>, 
                  "IndexIterator requires non-const, non-volatile type");

public:
    // STL iterator type definitions
    using iterator_category = std::input_iterator_tag;
    using value_type = T;
    using difference_type = std::ptrdiff_t;
    using pointer = T*;
    using reference = T&;

    // Constructors
    IndexIterator() noexcept : iter_handle_(Invalid::ROW_ITER), is_end_(true) {}
    
    /**
     * @brief Create iterator for exact value match on an index
     * 
     * Efficiently finds all rows where the indexed field exactly matches the given value.
     * Uses btree index scanning for O(log n) lookup + O(k) iteration over k matching rows.
     * 
     * @tparam FieldType The type of the indexed field (must match index column type)
     * @param index_id The index to scan (from table index declaration)
     * @param value The exact value to match
     * 
     * @note This constructor is typically called internally by ctx.db[field_accessor].filter(value).
     *       Developers should use the filter() API rather than constructing IndexIterator directly.
     * 
     * @example How developers use indexed queries (via filter API):
     * @code
     * SPACETIMEDB_TABLE(Player, players, Public);
     * FIELD_Index(players, level);  // Creates level index
     * 
     * // In a view - find all level 0 players using filter()
     * for (const auto& player : ctx.db[players_level].filter(0u)) {
     *     LOG_INFO("Found level 0 player: " + player.name);
     * }
     * @endcode
     */
    template<typename FieldType>
    IndexIterator(IndexId index_id, const FieldType& value) {
        // Serialize the exact value for point scan
        SpacetimeDB::bsatn::Writer point_writer;
        SpacetimeDB::bsatn::serialize(point_writer, value);
        auto point_buffer = point_writer.take_buffer();

        // Use optimized point scan for exact value matches
        Status status = FFI::datastore_index_scan_point_bsatn(
            index_id,
            point_buffer.data(), point_buffer.size(),
            &iter_handle_
        );
        
        if (status != StatusCode::OK) {
            std::abort(); // IndexIterator: datastore_index_scan_point_bsatn failed
        }
        advance();
    }
    
    /**
     * @brief Helper to serialize first N-1 elements of a tuple
     */
    template<typename Tuple, std::size_t... Is>
    static void serialize_tuple_prefix(SpacetimeDB::bsatn::Writer& writer, const Tuple& tuple, std::index_sequence<Is...>) {
        (serialize(writer, std::get<Is>(tuple)), ...);
    }

    /**
     * @brief Helper to serialize Range as rstart/rend bounds
     * Converts a Range<T> to the binary format expected by datastore_index_scan_range_bsatn
     */
    template<typename RangeT>
    static std::vector<uint8_t> serialize_range_start(const RangeT& range) {
        SpacetimeDB::bsatn::Writer writer;
        
        if (range.start) {
            // Inclusive bound
            writer.write_u8(0);  // BoundVariant::Inclusive
            serialize(writer, *range.start);
        } else {
            // Unbounded - use Unbounded variant
            writer.write_u8(2);  // BoundVariant::Unbounded
        }
        
        return writer.take_buffer();
    }

    template<typename RangeT>
    static std::vector<uint8_t> serialize_range_end(const RangeT& range) {
        SpacetimeDB::bsatn::Writer writer;
        
        if (range.end) {
            // Exclusive or Inclusive based on bound type
            uint8_t variant = (range.bound_type == RangeBound::Inclusive) ? 0 : 1;
            writer.write_u8(variant);  // BoundVariant::Inclusive(0) or Exclusive(1)
            serialize(writer, *range.end);
        } else {
            // Unbounded
            writer.write_u8(2);  // BoundVariant::Unbounded
        }
        
        return writer.take_buffer();
    }

    /**
     * @brief Create iterator for prefix-only match (N-1 columns specified)
     * 
     * Finds all rows where the first N-1 indexed columns match, regardless of the last column.
     * Useful for queries like "find all scores for player 123 at any level".
     * 
     * @tparam PrefixType The type of the first indexed column
     * @param index_id The multi-column index to scan
     * @param prefix_value Value to match for the prefix column
     * 
     * @example Prefix match - find all scores for a player:
     * @code
     * FIELD_NamedMultiColumnIndex(score, by_player_and_level, player_id, level)
     * 
     * // Find all scores for player 123 (any level)
     * auto scores = ctx.db[score_by_player_and_level].filter(uint32_t(123));
     * @endcode
     */
    template<typename PrefixType>
    IndexIterator(prefix_match_tag, IndexId index_id, const PrefixType& prefix_value)
        requires (!is_tuple_v<PrefixType> && !is_range_v<PrefixType>)
    {
        // Serialize prefix value
        SpacetimeDB::bsatn::Writer prefix_writer;
        serialize(prefix_writer, prefix_value);
        auto prefix_buffer = prefix_writer.take_buffer();

        // Create unbounded range for the remaining columns
        SpacetimeDB::bsatn::Writer rstart_writer, rend_writer;
        rstart_writer.write_u8(2);  // Unbounded
        rend_writer.write_u8(2);    // Unbounded
        auto rstart_buffer = rstart_writer.take_buffer();
        auto rend_buffer = rend_writer.take_buffer();

        // Call FFI with prefix_elems = 1 (only the first column)
        Status status = FFI::datastore_index_scan_range_bsatn(
            index_id,
            prefix_buffer.data(), prefix_buffer.size(), ColId{1},
            rstart_buffer.data(), rstart_buffer.size(),
            rend_buffer.data(), rend_buffer.size(),
            &iter_handle_
        );
        
        if (status != StatusCode::OK) {
            std::abort(); // IndexIterator: prefix-only match failed
        }
        advance();
    }

    /**
     * @brief Create iterator for prefix match with range on last column
     * 
     * Finds all rows where the first N-1 columns match exactly and the last column
     * falls within the specified range.
     * 
     * @example Range on last column - find scores for a player at specific levels:
     * @code
     * FIELD_NamedMultiColumnIndex(score, by_player_and_level, player_id, level)
     * 
     * // Find scores for player 123 at levels 1-10
     * auto scores = ctx.db[score_by_player_and_level].filter(
     *     std::make_tuple(uint32_t(123), range_inclusive(1u, 10u))
     * );
     * @endcode
     */
    template<typename PrefixType, typename RangeType>
    IndexIterator(IndexId index_id, const std::tuple<PrefixType, RangeType>& values)
        requires (is_range_v<RangeType>)
    {
        // Serialize prefix value
        SpacetimeDB::bsatn::Writer prefix_writer;
        serialize(prefix_writer, std::get<0>(values));
        auto prefix_buffer = prefix_writer.take_buffer();

        // Serialize range as start/end bounds
        const auto& range = std::get<1>(values);
        auto rstart_buffer = serialize_range_start(range);
        auto rend_buffer = serialize_range_end(range);

        // Call FFI with prefix_elems = 1 (only the prefix column)
        Status status = FFI::datastore_index_scan_range_bsatn(
            index_id,
            prefix_buffer.data(), prefix_buffer.size(), ColId{1},
            rstart_buffer.data(), rstart_buffer.size(),
            rend_buffer.data(), rend_buffer.size(),
            &iter_handle_
        );
        
        if (status != StatusCode::OK) {
            std::abort(); // IndexIterator: prefix+range match failed
        }
        advance();
    }

    /**
     * @brief Create iterator for multi-column exact match
     * 
     * Efficiently finds all rows where all indexed columns exactly match the tuple values.
     * 
     * @tparam FieldTypes The types of the indexed fields
     * @param index_id The multi-column index to scan
     * @param values Tuple of values to match (one per column)
     * 
     * @example Multi-column exact match:
     * @code
     * FIELD_NamedMultiColumnIndex(score, by_player_and_level, player_id, level)
     * 
     * // Find exact score for player 123 at level 5
     * auto iter = IndexIterator<Score>(index_id, std::make_tuple(uint32_t(123), uint32_t(5)));
     * @endcode
     */
    template<typename... FieldTypes>
    IndexIterator(IndexId index_id, const std::tuple<FieldTypes...>& values) 
        requires (sizeof...(FieldTypes) > 1 && sizeof...(FieldTypes) <= 6)
    {
        constexpr std::size_t N = sizeof...(FieldTypes);
        constexpr std::size_t prefix_count = N - 1;
        
        // Serialize first N-1 elements into prefix buffer
        SpacetimeDB::bsatn::Writer prefix_writer;
        serialize_tuple_prefix(prefix_writer, values, std::make_index_sequence<prefix_count>{});
        auto prefix_buffer = prefix_writer.take_buffer();
        
        // Serialize the last element as both start and end bounds (exact match)
        SpacetimeDB::bsatn::Writer bound_writer;
        bound_writer.write_u8(0);  // Bound::Included
        serialize(bound_writer, std::get<N - 1>(values));  // Last element only
        auto bound_buffer = bound_writer.take_buffer();

        // Call FFI with prefix_elems = N-1 (as per C# pattern)
        Status status = FFI::datastore_index_scan_range_bsatn(
            index_id,
            prefix_buffer.data(), prefix_buffer.size(), ColId{static_cast<uint16_t>(prefix_count)},
            bound_buffer.data(), bound_buffer.size(),  // Last value as start
            bound_buffer.data(), bound_buffer.size(),  // Last value as end
            &iter_handle_
        );
        
        if (status != StatusCode::OK) {
            std::abort(); // IndexIterator: multi-column exact match failed
        }
        advance();
    }
    
    /**
     * @brief Create iterator for range query on an index
     * 
     * Efficiently iterates over rows where the indexed field falls within a specified range.
     * Supports inclusive and exclusive bounds, unbounded ranges, and custom types.
     * 
     * @tparam FieldType The type of the indexed field
     * @param index_id The index to scan
     * @param range The range specification (start, end, bound type)
     * 
     * @note This constructor is typically called internally by ctx.db[field_accessor].filter(range).
     *       Developers should use the filter() API with range helper functions rather than
     *       constructing IndexIterator or Range objects directly.
     * 
     * @example How developers use range queries (via filter API):
     * @code
     * FIELD_Index(person, age);
     * 
     * // Find persons aged 25-30 using range_inclusive()
     * auto age_range = range_inclusive(uint8_t(25), uint8_t(30));
     * for (const auto& person : ctx.db[person_age].filter(age_range)) {
     *     LOG_INFO("Person in range: " + person.name);
     * }
     * 
     * // Find persons 18 and older using range_from()
     * auto adult_range = range_from(uint8_t(18));
     * size_t adult_count = ctx.db[person_age].filter(adult_range).size();
     * 
     * // Find persons under 30 using range_to()
     * auto young_range = range_to(uint8_t(30));
     * for (const auto& person : ctx.db[person_age].filter(young_range)) {
     *     LOG_INFO("Young person: " + person.name);
     * }
     * @endcode
     * 
     * @see range_from, range_to, range_inclusive, range_to_inclusive for creating ranges
     */
    template<typename FieldType>
    IndexIterator(IndexId index_id, const Range<FieldType>& range) {
        std::vector<uint8_t> start_buffer;
        std::vector<uint8_t> end_buffer;

        // Serialize range bounds if present
        if (range.start.has_value()) {
            SpacetimeDB::bsatn::Writer start_writer;
            start_writer.write_u8(0);
            SpacetimeDB::bsatn::serialize(start_writer, range.start.value());
            start_buffer = start_writer.take_buffer();
        } else {
            start_buffer.push_back(2); // Bound::Unbounded tag
        }
        
        if (range.end.has_value()) {
            SpacetimeDB::bsatn::Writer end_writer;
            uint8_t end_tag = (range.bound_type == RangeBound::Inclusive) ? 0 : 1;
            end_writer.write_u8(end_tag);
            SpacetimeDB::bsatn::serialize(end_writer, range.end.value());
            end_buffer = end_writer.take_buffer();
        } else {
            end_buffer.push_back(2); // Bound::Unbounded tag
        }
        
        // Call range scan with no prefix (range queries on single column)
        Status status = FFI::datastore_index_scan_range_bsatn(
            index_id,
            nullptr, 0, ColId{0},  // no prefix for range queries
            start_buffer.empty() ? nullptr : start_buffer.data(), start_buffer.size(),
            end_buffer.empty() ? nullptr : end_buffer.data(), end_buffer.size(),
            &iter_handle_
        );
        
        if (status != StatusCode::OK) {
            std::abort(); // IndexIterator: datastore_index_scan_range_bsatn failed
        }
        
        // Apply inclusive/exclusive bounds during iteration
        bound_type_ = range.bound_type;
        // Note: bounds are handled by the btree scan itself
        
        advance();
    }

    ~IndexIterator() noexcept {
        if (iter_handle_ != Invalid::ROW_ITER && !ffi_exhausted_) {
            FFI::row_iter_bsatn_close(iter_handle_);
        }
    }

    // Move-only semantics
    IndexIterator(const IndexIterator&) = delete;
    IndexIterator& operator=(const IndexIterator&) = delete;
    
    IndexIterator(IndexIterator&& other) noexcept : IndexIterator() {
        swap(*this, other);
    }
    
    IndexIterator& operator=(IndexIterator&& other) noexcept {
        if (this != &other) {
            IndexIterator temp(std::move(other));
            swap(*this, temp);
        }
        return *this;
    }
    
    friend void swap(IndexIterator& a, IndexIterator& b) noexcept {
        using std::swap;
        swap(a.iter_handle_, b.iter_handle_);
        swap(a.row_buffer_, b.row_buffer_);
        swap(a.current_batch_, b.current_batch_);
        swap(a.current_index_, b.current_index_);
        swap(a.current_row_, b.current_row_);
        swap(a.is_valid_, b.is_valid_);
        swap(a.is_end_, b.is_end_);
        swap(a.ffi_exhausted_, b.ffi_exhausted_);
        swap(a.bound_type_, b.bound_type_);
    }

    // Iterator operations
    const T& operator*() const {
        if (!is_valid_) std::abort();
        return current_row_;
    }
    
    const T* operator->() const { return &**this; }
    
    IndexIterator& operator++() {
        advance();
        return *this;
    }
    
    bool operator==(const IndexIterator& other) const noexcept {
        return is_valid_ == other.is_valid_;
    }
    
    bool operator!=(const IndexIterator& other) const noexcept {
        return !(*this == other);
    }
    
    // Range-based for loop support
    // Returns self to support: for (auto& row : ctx.db[field].filter(value)) { ... }
    // This works because the temporary returned from filter() stays alive for the loop
    IndexIterator& begin() noexcept { return *this; }
    IndexIterator end() const noexcept { return IndexIterator(); }
    
    // Explicitly allow const iteration - needed for range-based for loops
    // The iterator itself is const-iterable (doesn't modify internal state during iteration)
    const IndexIterator& cbegin() const noexcept { return *this; }
    const IndexIterator cend() const noexcept { return IndexIterator(); }
    
    /**
     * @brief Collect all remaining results into a vector
     * 
     * Convenient method to materialize all matching rows from the iterator
     * into a std::vector without manual iteration.
     * 
     * @return Vector containing all matching rows
     */
    std::vector<T> collect() {
        std::vector<T> results;
        while (is_valid_) {
            results.push_back(std::move(current_row_));
            advance();
        }
        return results;
    }

private:
    RowIter iter_handle_ = Invalid::ROW_ITER;
    std::vector<uint8_t> row_buffer_;
    std::vector<T> current_batch_;
    size_t current_index_ = 0;
    T current_row_;
    bool is_valid_ = false;
    bool is_end_ = false;
    bool ffi_exhausted_ = false;
    
    // For handling inclusive/exclusive bounds
    RangeBound bound_type_ = RangeBound::Exclusive;
    // Note: end_value_ tracking would require knowing the field type
    // For now we rely on btree scan to handle bounds correctly
    
    // Constants for performance tuning
    static constexpr size_t INITIAL_ROW_BUFFER_SIZE = 4096;
    static constexpr size_t MAX_ROW_BUFFER_SIZE = 1024 * 1024;
    static constexpr size_t TYPICAL_BATCH_SIZE = 32;
    static constexpr int16_t ITER_EXHAUSTED = -1;
    static constexpr int16_t ITER_OK = 0;
    static constexpr uint16_t ERROR_BUFFER_TOO_SMALL = 3;
    
    // Helper to serialize tuple elements without treating tuple as a type
    template<typename... FieldTypes, std::size_t... Is>
    static void serialize_tuple_elements(SpacetimeDB::bsatn::Writer& writer, 
                                        const std::tuple<FieldTypes...>& values,
                                        std::index_sequence<Is...>) {
        (SpacetimeDB::bsatn::serialize(writer, std::get<Is>(values)), ...);
    }
    
    void advance() {
        if (is_end_) {
            is_valid_ = false;
            return;
        }
        
        // Try current batch first
        if (current_index_ < current_batch_.size()) {
            current_row_ = std::move(current_batch_[current_index_++]);
            is_valid_ = true;
            return;
        }
        
        // Need new batch - but check if FFI is already exhausted
        if (ffi_exhausted_) {
            is_end_ = true;
            is_valid_ = false;
            return;
        }
        
        // Fetch new batch
        fetch_batch();
        
        // Try again with new batch
        if (current_index_ < current_batch_.size()) {
            current_row_ = std::move(current_batch_[current_index_++]);
            is_valid_ = true;
        } else {
            is_end_ = true;
            is_valid_ = false;
        }
    }
    
    void fetch_batch() {
        row_buffer_.resize(INITIAL_ROW_BUFFER_SIZE);
        size_t buffer_len = row_buffer_.size();
        
        int16_t ret = FFI::row_iter_bsatn_advance(iter_handle_, 
                                                  row_buffer_.data(), 
                                                  &buffer_len);
        
        if (ret == ITER_EXHAUSTED) {
            ffi_exhausted_ = true;
            if (buffer_len > 0) {
                row_buffer_.resize(buffer_len);
                deserialize_batch(buffer_len);
            }
            return;
        }
        
        if (ret == ERROR_BUFFER_TOO_SMALL) {
            if (buffer_len > MAX_ROW_BUFFER_SIZE) {
                std::abort(); // Buffer size exceeds maximum
            }
            row_buffer_.resize(buffer_len);
            ret = FFI::row_iter_bsatn_advance(iter_handle_, 
                                             row_buffer_.data(), 
                                             &buffer_len);
        }
        
        if (ret > 0) {
            std::abort(); // IndexIterator: row_iter_bsatn_advance failed
        }
        
        row_buffer_.resize(buffer_len);
        deserialize_batch(buffer_len);
    }
    
    void deserialize_batch(size_t buffer_len) {
        current_batch_.clear();
        current_batch_.reserve(TYPICAL_BATCH_SIZE);
        current_index_ = 0;
        
        if (buffer_len == 0) return;
        
        SpacetimeDB::bsatn::Reader reader(row_buffer_.data(), buffer_len);
        while (!reader.is_eos()) {
            // Without exceptions, deserialization failures will abort
            current_batch_.emplace_back(SpacetimeDB::bsatn::deserialize<T>(reader));
        }
    }
};

// =============================================================================
// Range Wrapper for clean range-based for loops
// =============================================================================

/**
 * @brief Lightweight wrapper to make IndexIterator work with range-based for loops
 * 
 * Provides the range interface while holding the move-only IndexIterator,
 * allowing clean syntax: for (auto& row : ctx.db[field].filter(value)) { ... }
 */
template<typename T>
class IndexIteratorRange {
private:
    struct Iterator {
        std::shared_ptr<IndexIterator<T>> iter;
        bool is_end = true;

        Iterator() = default;
        Iterator(std::shared_ptr<IndexIterator<T>> it, bool end) noexcept
            : iter(std::move(it)), is_end(end) {}

        const T& operator*() const noexcept { return **iter; }
        const T* operator->() const noexcept { return iter->operator->(); }

        Iterator& operator++() {
            ++(*iter);
            if (*iter == IndexIterator<T>()) {
                is_end = true;
                iter.reset();
            }
            return *this;
        }

        bool operator==(const Iterator& other) const noexcept {
            if (is_end && other.is_end) return true;
            return is_end == other.is_end && iter == other.iter;
        }

        bool operator!=(const Iterator& other) const noexcept { return !(*this == other); }
    };

    std::shared_ptr<IndexIterator<T>> iter_;
    
public:
    explicit IndexIteratorRange(IndexIterator<T>&& it) noexcept
        : iter_(std::make_shared<IndexIterator<T>>(std::move(it))) {}

    Iterator begin() {
        if (!iter_ || *iter_ == IndexIterator<T>()) {
            return Iterator(nullptr, true);
        }
        return Iterator(iter_, false);
    }

    Iterator end() { return Iterator(nullptr, true); }

    /**
     * @brief Materialize all remaining results into a vector
     * 
     * Convenience method to collect all matching rows without manual iteration.
     * 
     * @return Vector containing all matching rows
     */
    std::vector<T> collect() {
        if (!iter_) return {};
        return iter_->collect();
    }

    /**
     * @brief Count all remaining results
     *
     * Note: This consumes the iterator, just like iterating in a for-loop.
     */
    size_t size() {
        size_t count = 0;
        for (auto it = begin(); it != end(); ++it) {
            ++count;
        }
        return count;
    }

    /**
     * @brief Alias for size()
     *
     * Note: This consumes the iterator, just like iterating in a for-loop.
     */
    size_t count() { return size(); }
};
} // namespace SpacetimeDB


#endif // SPACETIMEDB_INDEX_ITERATOR_H