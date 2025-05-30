#ifndef SPACETIMEDB_SDK_TABLE_H
#define SPACETIMEDB_SDK_TABLE_H

#include "spacetimedb_sdk_types.h"
#include "bsatn.h"
#include "spacetimedb_abi.h" // For ABI function calls

#include <string>
#include <vector>
#include <stdexcept> // For std::runtime_error
#include <memory>    // For std::unique_ptr in iterator if needed
#include <iostream>  // For temporary debugging if needed

namespace spacetimedb {
namespace sdk {

// Forward declare Table for TableIterator friending or use.
template<typename T>
class Table;

template<typename T>
class TableIterator {
public:
    // Standard iterator typedefs
    using iterator_category = std::input_iterator_tag;
    using value_type = T;
    using difference_type = std::ptrdiff_t;
    using pointer = T*;
    using reference = T&;

    // Default constructor for end iterator
    TableIterator() : iter_handle_(0), is_valid_(false) {}

    // Constructor called by Table
    explicit TableIterator(BufferIter iter_handle) 
        : iter_handle_(iter_handle), is_valid_(false) {
        if (iter_handle_ != 0) { // 0 could be an invalid iterator handle
            advance(); // Load the first element
        }
    }

    TableIterator(const TableIterator& other) = delete; // No copy constructor
    TableIterator& operator=(const TableIterator& other) = delete; // No copy assignment

    TableIterator(TableIterator&& other) noexcept 
        : iter_handle_(other.iter_handle_), 
          current_row_(std::move(other.current_row_)), 
          is_valid_(other.is_valid_) {
        other.iter_handle_ = 0; // Invalidate other
        other.is_valid_ = false;
    }

    TableIterator& operator=(TableIterator&& other) noexcept {
        if (this != &other) {
            if (iter_handle_ != 0) { 
                 _iter_drop(iter_handle_); // Ignore error code on drop in move assignment
            }
            iter_handle_ = other.iter_handle_;
            current_row_ = std::move(other.current_row_);
            is_valid_ = other.is_valid_;
            other.iter_handle_ = 0;
            other.is_valid_ = false;
        }
        return *this;
    }

    ~TableIterator() {
        if (iter_handle_ != 0) {
            _iter_drop(iter_handle_); // Ignore error code on drop in destructor
            iter_handle_ = 0;
        }
    }

    const T& operator*() const {
        if (!is_valid_) {
            throw std::out_of_range("Dereferencing invalid or end TableIterator");
        }
        return current_row_;
    }

    const T* operator->() const {
        if (!is_valid_) {
            throw std::out_of_range("Dereferencing invalid or end TableIterator");
        }
        return &current_row_;
    }

    TableIterator& operator++() {
        if (!is_valid_) { // Cannot advance an invalid/end iterator
             throw std::out_of_range("Incrementing invalid or end TableIterator");
        }
        advance();
        return *this;
    }
    
    // For range-based for loops, comparison with end iterator
    bool operator!=(const TableIterator& other) const {
        // True if one is valid and the other is not.
        // If both are valid, they are different if their handles are different (though not strictly necessary for end iterator comparison)
        // If both are invalid (both end iterators), they are equal, so `!=` is false.
        return is_valid_ != other.is_valid_; 
    }

    bool operator==(const TableIterator& other) const {
        return is_valid_ == other.is_valid_;
    }

private:
    void advance() {
        if (iter_handle_ == 0) { 
            is_valid_ = false;
            return;
        }

        Buffer row_data_buffer_handle = 0;
        // ABI: _iter_next(BufferIter iter_handle, Buffer *out_row_data_buf_ptr) returns uint16_t error_code
        uint16_t error_code = _iter_next(iter_handle_, &row_data_buffer_handle);

        if (error_code != 0) {
            is_valid_ = false; 
            throw std::runtime_error("TableIterator: _iter_next failed with code " + std::to_string(error_code));
        }

        if (row_data_buffer_handle == 0) { // End of iteration
            is_valid_ = false;
            // Consider iter_handle_ spent, though destructor handles freeing.
            // If ABI implies iter_handle is invalid after returning 0 for buffer, we could _iter_drop here.
            // For now, rely on destructor for cleanup.
            return;
        }

        size_t len = _buffer_len(row_data_buffer_handle);
        std::vector<uint8_t> temp_buffer(len);
        
        uint16_t consume_error_code = 0;
        if (len > 0) {
            consume_error_code = _buffer_consume(row_data_buffer_handle, temp_buffer.data(), temp_buffer.size());
        } else { // Consume (and thus free) the buffer handle even if it's empty
            consume_error_code = _buffer_consume(row_data_buffer_handle, nullptr, 0);
        }

        if (consume_error_code != 0) {
            is_valid_ = false;
            throw std::runtime_error("TableIterator: _buffer_consume failed with code " + std::to_string(consume_error_code));
        }
        
        try {
            // Even if len is 0, try to deserialize. T's bsatn_deserialize should handle empty input if it's valid for T.
            bsatn::bsatn_reader reader(temp_buffer.data(), temp_buffer.size());
            current_row_.bsatn_deserialize(reader); 
            is_valid_ = true;
        } catch (const std::exception& e) {
            is_valid_ = false;
            // Propagate deserialization error, possibly wrapping it
            throw std::runtime_error(std::string("TableIterator: BSATN deserialization failed: ") + e.what());
        }
    }

    BufferIter iter_handle_;
    T current_row_; 
    bool is_valid_;     
};


template<typename T>
class Table {
public:
    explicit Table(uint32_t table_id) : table_id_(table_id) {
        static_assert(std::is_base_of_v<bsatn::BsatnSerializable, T> ||
                      (requires(T& t, bsatn::bsatn_writer& w) { t.bsatn_serialize(w); } &&
                       requires(T& t, bsatn::bsatn_reader& r) { t.bsatn_deserialize(r); }),
                      "Table type T must implement BsatnSerializable or provide bsatn_serialize/bsatn_deserialize methods.");
    }

    void insert(T& row_data) { // row_data is non-const to allow modification by host (e.g. PK)
        bsatn::bsatn_writer writer;
        row_data.bsatn_serialize(writer);
        
        std::vector<uint8_t> buffer_vec = writer.move_buffer();
        
        // ABI: _insert(uint32_t table_id, uint8_t *row_bsatn_ptr, size_t row_bsatn_len) returns uint16_t error_code
        // The `row_bsatn_ptr` (buffer_vec.data()) is mutable and its content can be updated by the host.
        uint16_t error_code = _insert(table_id_, buffer_vec.data(), buffer_vec.size());

        if (error_code != 0) {
            throw std::runtime_error("Table::insert: _insert ABI call failed with code " + std::to_string(error_code));
        }

        // Deserialize back from the potentially modified buffer_vec
        try {
            bsatn::bsatn_reader reader(buffer_vec.data(), buffer_vec.size());
            row_data.bsatn_deserialize(reader);
        } catch (const std::exception& e) {
            // It's possible the host didn't modify the buffer, or only modified part of it.
            // If deserialization fails here, it might indicate an issue with the host's modification
            // or that the buffer length was also changed by the host without us knowing.
            // The current ABI for _insert doesn't return the new size. Assume size is constant.
            throw std::runtime_error(std::string("Table::insert: BSATN deserialization after insert failed: ") + e.what());
        }
    }

    template<typename ValueType>
    uint32_t delete_by_col_eq(uint32_t column_index, const ValueType& value_to_match) {
        static_assert(
            (std::is_arithmetic_v<ValueType> || std::is_same_v<ValueType, bool> || std::is_same_v<ValueType, std::string>) ||
            (std::is_base_of_v<bsatn::BsatnSerializable, ValueType> || requires(const ValueType& v, bsatn::bsatn_writer& w) { v.bsatn_serialize(w); }),
            "ValueType for delete_by_col_eq must be a supported primitive, std::string, or implement bsatn_serialize."
        );

        bsatn::bsatn_writer writer;
        if constexpr (std::is_same_v<ValueType, bool>) writer.write_bool(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint8_t>) writer.write_u8(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint16_t>) writer.write_u16(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint32_t>) writer.write_u32(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint64_t>) writer.write_u64(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int8_t>) writer.write_i8(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int16_t>) writer.write_i16(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int32_t>) writer.write_i32(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int64_t>) writer.write_i64(value_to_match);
        else if constexpr (std::is_same_v<ValueType, float>) writer.write_f32(value_to_match);
        else if constexpr (std::is_same_v<ValueType, double>) writer.write_f64(value_to_match);
        else if constexpr (std::is_same_v<ValueType, std::string>) writer.write_string(value_to_match);
        else { 
            value_to_match.bsatn_serialize(writer);
        }
        
        std::vector<uint8_t> value_buffer_vec = writer.move_buffer();
        uint32_t deleted_count = 0;

        // ABI: _delete_by_col_eq(uint32_t table_id, uint32_t col_id, const uint8_t *value_bsatn_ptr, size_t value_bsatn_len, uint32_t *out_deleted_count_ptr)
        uint16_t error_code = _delete_by_col_eq(table_id_, column_index, value_buffer_vec.data(), value_buffer_vec.size(), &deleted_count);

        if (error_code != 0) {
            throw std::runtime_error("Table::delete_by_col_eq: _delete_by_col_eq ABI call failed with code " + std::to_string(error_code));
        }
        return deleted_count;
    }

    TableIterator<T> iter() {
        BufferIter iter_handle = 0;
        // ABI: _iter_start(uint32_t table_id, BufferIter *out_iter_ptr)
        uint16_t error_code = _iter_start(table_id_, &iter_handle);
        if (error_code != 0) {
            throw std::runtime_error("Table::iter: _iter_start ABI call failed with code " + std::to_string(error_code));
        }
        // If iter_handle is 0 after a successful call, it means the iterator is empty from the start.
        // The TableIterator constructor will handle this by not calling advance() if handle is 0,
        // or advance() will correctly set is_valid_ to false if _iter_next returns an empty/invalid buffer.
        return TableIterator<T>(iter_handle);
    }

    template<typename ValueType>
    std::vector<T> find_by_col_eq(uint32_t column_index, const ValueType& value_to_match) {
         static_assert(
            (std::is_arithmetic_v<ValueType> || std::is_same_v<ValueType, bool> || std::is_same_v<ValueType, std::string>) ||
            (std::is_base_of_v<bsatn::BsatnSerializable, ValueType> || requires(const ValueType& v, bsatn::bsatn_writer& w) { v.bsatn_serialize(w); }),
            "ValueType for find_by_col_eq must be a supported primitive, std::string, or implement bsatn_serialize."
        );

        bsatn::bsatn_writer writer;
        if constexpr (std::is_same_v<ValueType, bool>) writer.write_bool(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint8_t>) writer.write_u8(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint16_t>) writer.write_u16(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint32_t>) writer.write_u32(value_to_match);
        else if constexpr (std::is_same_v<ValueType, uint64_t>) writer.write_u64(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int8_t>) writer.write_i8(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int16_t>) writer.write_i16(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int32_t>) writer.write_i32(value_to_match);
        else if constexpr (std::is_same_v<ValueType, int64_t>) writer.write_i64(value_to_match);
        else if constexpr (std::is_same_v<ValueType, float>) writer.write_f32(value_to_match);
        else if constexpr (std::is_same_v<ValueType, double>) writer.write_f64(value_to_match);
        else if constexpr (std::is_same_v<ValueType, std::string>) writer.write_string(value_to_match);
        else { 
            value_to_match.bsatn_serialize(writer);
        }
        
        std::vector<uint8_t> value_buffer_vec = writer.move_buffer();
        Buffer result_buffer_handle = 0; // This will receive the handle to the buffer with concatenated rows

        // ABI: _iter_by_col_eq(uint32_t table_id, uint32_t col_id, const uint8_t *value_bsatn_ptr, size_t value_bsatn_len, Buffer *out_buffer_ptr_with_rows)
        uint16_t error_code = _iter_by_col_eq(table_id_, column_index, value_buffer_vec.data(), value_buffer_vec.size(), &result_buffer_handle);

        if (error_code != 0) {
            throw std::runtime_error("Table::find_by_col_eq: _iter_by_col_eq ABI call failed with code " + std::to_string(error_code));
        }

        std::vector<T> results;
        if (result_buffer_handle == 0) { // No results or error already thrown.
            return results;
        }

        size_t len = _buffer_len(result_buffer_handle);
        std::vector<uint8_t> concatenated_rows_buffer(len);
        
        uint16_t consume_error_code = 0;
        if (len > 0) {
             consume_error_code = _buffer_consume(result_buffer_handle, concatenated_rows_buffer.data(), concatenated_rows_buffer.size());
        } else { // Consume (and thus free) the buffer handle even if it's empty
             consume_error_code = _buffer_consume(result_buffer_handle, nullptr, 0);
        }

        if (consume_error_code != 0) {
            // Note: result_buffer_handle might not be valid anymore if _buffer_consume failed mid-way
            // or if it always frees. Best not to try freeing it again.
            throw std::runtime_error("Table::find_by_col_eq: _buffer_consume failed with code " + std::to_string(consume_error_code));
        }

        if (len > 0) {
            bsatn::bsatn_reader reader(concatenated_rows_buffer.data(), concatenated_rows_buffer.size());
            try {
                while(!reader.eof()) {
                    T row_data;
                    row_data.bsatn_deserialize(reader); 
                    results.push_back(std::move(row_data));
                }
            } catch (const std::exception& e) {
                // This could happen if the buffer doesn't contain an exact number of T objects
                // or if deserialization of an object fails.
                throw std::runtime_error(std::string("Table::find_by_col_eq: BSATN deserialization of concatenated rows failed: ") + e.what());
            }
        }
        return results;
    }

private:
    uint32_t table_id_;
};

} // namespace sdk
} // namespace spacetimedb

#endif // SPACETIMEDB_SDK_TABLE_H
