#ifndef SPACETIMEDB_SDK_TABLE_H
#define SPACETIMEDB_SDK_TABLE_H

#include <spacetimedb/sdk/spacetimedb_sdk_types.h>
#include <spacetimedb/bsatn/bsatn.h>
#include <spacetimedb/abi/spacetimedb_abi.h> // For ABI function calls

#include <string>
#include <vector>
#include <stdexcept> // For std::runtime_error
#include <memory>    // For std::unique_ptr in iterator if needed
// #include <iostream>  // For temporary debugging if needed (remove for final)

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
                 uint16_t drop_ec = _iter_drop(iter_handle_);
                 // Optionally log drop_ec if not 0, though in move assignment errors are hard to propagate
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
            _iter_drop(iter_handle_); // Error code ignored in destructor as exceptions shouldn't escape
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

    bool operator!=(const TableIterator& other) const {
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
        uint16_t error_code = _iter_next(iter_handle_, &row_data_buffer_handle);

        if (error_code != 0) {
            is_valid_ = false;
            throw std::runtime_error("TableIterator: _iter_next failed with code " + std::to_string(error_code));
        }

        if (row_data_buffer_handle == 0) { // Standard way to signal end of iteration
            is_valid_ = false;
            return;
        }

        size_t len = _buffer_len(row_data_buffer_handle);
        std::vector<uint8_t> temp_buffer(len);

        uint16_t consume_error_code = _buffer_consume(row_data_buffer_handle, temp_buffer.data(), len);

        if (consume_error_code != 0) {
            is_valid_ = false;
            throw std::runtime_error("TableIterator: _buffer_consume failed with code " + std::to_string(consume_error_code));
        }

        try {
            bsatn::bsatn_reader reader(temp_buffer.data(), temp_buffer.size());
            current_row_.bsatn_deserialize(reader);
            is_valid_ = true;
        } catch (const std::exception& e) {
            is_valid_ = false;
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

    void insert(T& row_data) {
        bsatn::bsatn_writer writer;
        row_data.bsatn_serialize(writer);

        std::vector<uint8_t> buffer_vec = writer.move_buffer();

        uint16_t error_code = _insert(table_id_, buffer_vec.data(), buffer_vec.size());

        if (error_code != 0) {
            throw std::runtime_error("Table::insert: _insert ABI call failed with code " + std::to_string(error_code));
        }

        try {
            bsatn::bsatn_reader reader(buffer_vec.data(), buffer_vec.size());
            row_data.bsatn_deserialize(reader);
        } catch (const std::exception& e) {
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

        uint16_t error_code = _delete_by_col_eq(table_id_, column_index, value_buffer_vec.data(), value_buffer_vec.size(), &deleted_count);

        if (error_code != 0) {
            throw std::runtime_error("Table::delete_by_col_eq: _delete_by_col_eq ABI call failed with code " + std::to_string(error_code));
        }
        return deleted_count;
    }

    TableIterator<T> iter() {
        BufferIter iter_handle = 0;
        uint16_t error_code = _iter_start(table_id_, &iter_handle);
        if (error_code != 0) {
            throw std::runtime_error("Table::iter: _iter_start ABI call failed with code " + std::to_string(error_code));
        }
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
        Buffer result_buffer_handle = 0;

        uint16_t error_code = _iter_by_col_eq(table_id_, column_index, value_buffer_vec.data(), value_buffer_vec.size(), &result_buffer_handle);

        if (error_code != 0) {
            throw std::runtime_error("Table::find_by_col_eq: _iter_by_col_eq ABI call failed with code " + std::to_string(error_code));
        }

        std::vector<T> results;
        if (result_buffer_handle == 0) {
            return results;
        }

        size_t len = _buffer_len(result_buffer_handle);
        std::vector<uint8_t> concatenated_rows_buffer(len);

        uint16_t consume_error_code = _buffer_consume(result_buffer_handle, concatenated_rows_buffer.data(), len);

        if (consume_error_code != 0) {
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
