#ifndef SPACETIMEDB_INDEX_ITERATOR_H
#define SPACETIMEDB_INDEX_ITERATOR_H

/**
 * @file index_iterator.h
 * @brief Index-based iteration for SpacetimeDB C++ SDK
 * 
 * Provides efficient index-based iteration using btree scans,
 * matching the performance of Rust's index-based filtering.
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
#include <stdexcept>
#include <optional>
#include <algorithm>
#include <utility>

namespace SpacetimeDb {

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
    
    // Constructor for exact value match
    template<typename FieldType>
    IndexIterator(IndexId index_id, const FieldType& value) {
        // Serialize the exact value for prefix matching
        SpacetimeDb::bsatn::Writer bound_writer;
        bound_writer.write_u8(0);  // Bound::Included tag
        SpacetimeDb::bsatn::serialize(bound_writer, value);
        auto bound_buffer = bound_writer.take_buffer();

        // For exact match, use the value as both prefix and range bounds
        Status status = FFI::datastore_btree_scan_bsatn(
            index_id,
            nullptr, 0, ColId{0},  // prefix with 1 element
            bound_buffer.data(), bound_buffer.size(),  // no range start
            bound_buffer.data(), bound_buffer.size(),  // no range end
            &iter_handle_
        );
        
        if (status != StatusCode::OK) {
            std::abort(); // IndexIterator: datastore_btree_scan_bsatn failed
        }
        advance();
    }
    
    // Constructor for range queries
    template<typename FieldType>
    IndexIterator(IndexId index_id, const Range<FieldType>& range) {
        std::vector<uint8_t> start_buffer;
        std::vector<uint8_t> end_buffer;

        // Serialize range bounds if present
        if (range.start.has_value()) {
            SpacetimeDb::bsatn::Writer start_writer;
            start_writer.write_u8(0);
            SpacetimeDb::bsatn::serialize(start_writer, range.start.value());
            start_buffer = start_writer.take_buffer();
        } else {
            start_buffer.push_back(2); // Bound::Unbounded tag
        }
        
        if (range.end.has_value()) {
            SpacetimeDb::bsatn::Writer end_writer;
            uint8_t end_tag = (range.bound_type == RangeBound::Inclusive) ? 0 : 1;
            end_writer.write_u8(end_tag);
            SpacetimeDb::bsatn::serialize(end_writer, range.end.value());
            end_buffer = end_writer.take_buffer();
        } else {
            end_buffer.push_back(2); // Bound::Unbounded tag
        }
        
        // Call btree scan with range bounds
        Status status = FFI::datastore_btree_scan_bsatn(
            index_id,
            nullptr, 0, ColId{0},  // no prefix for range queries
            start_buffer.empty() ? nullptr : start_buffer.data(), start_buffer.size(),
            end_buffer.empty() ? nullptr : end_buffer.data(), end_buffer.size(),
            &iter_handle_
        );
        
        if (status != StatusCode::OK) {
            std::abort(); // IndexIterator: datastore_btree_scan_bsatn failed
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
        
        SpacetimeDb::bsatn::Reader reader(row_buffer_.data(), buffer_len);
        while (!reader.is_eos()) {
            // Without exceptions, deserialization failures will abort
            current_batch_.emplace_back(SpacetimeDb::bsatn::deserialize<T>(reader));
        }
    }
};

} // namespace SpacetimeDb

#endif // SPACETIMEDB_INDEX_ITERATOR_H