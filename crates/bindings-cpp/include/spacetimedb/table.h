#ifndef SPACETIMEDB_LIBRARY_TABLE_H
#define SPACETIMEDB_LIBRARY_TABLE_H

/**
 * @file table.h
 * @brief Core table operations for SpacetimeDB C++ bindings
 * 
 * Provides type-safe Table<T> class and TableIterator<T> for CRUD operations
 * on SpacetimeDB tables with efficient batch iteration and STL compatibility.
 */

#include "spacetimedb/bsatn/types.h"
#include <spacetimedb/bsatn/bsatn.h>
#include <spacetimedb/abi/FFI.h>
#include <spacetimedb/bsatn/reader.h>
#include <spacetimedb/bsatn/writer.h>
#include <spacetimedb/bsatn/traits.h>
#include <spacetimedb/logger.h>

#include <string>
#include <vector>
#include <stdexcept>
#include <optional>
#include <algorithm>
#include <utility>

namespace SpacetimeDB {

// Forward declarations
template<typename T> class Table;
template<typename T> class TableIterator;

// =============================================================================
// Insert Error Handling
// =============================================================================

/**
 * Error types for insert operations
 */
enum class InsertErrorType {
    UniqueConstraintViolation,
    AutoIncOverflow,
    Other
};

/**
 * Insert error details
 */
struct InsertError {
    InsertErrorType type;
    Status status_code;
    std::string message;
    
    InsertError(InsertErrorType t, Status s, const std::string& msg) 
        : type(t), status_code(s), message(msg) {}
};

/**
 * Result type for try_insert operations
 */
template<typename T>
class InsertResult {
private:
    std::variant<T, InsertError> result_;
    
public:
    InsertResult(T&& value) : result_(std::move(value)) {}
    InsertResult(const InsertError& error) : result_(error) {}
    
    bool is_ok() const { return std::holds_alternative<T>(result_); }
    bool is_err() const { return !is_ok(); }
    
    const T& ok() const { return std::get<T>(result_); }
    T&& take_ok() { return std::move(std::get<T>(result_)); }
    
    const InsertError& err() const { return std::get<InsertError>(result_); }
};

// =============================================================================
// Implementation Details
// =============================================================================

namespace detail {
    // Performance tuning constants
    constexpr size_t INITIAL_ROW_BUFFER_SIZE = 128 * 1024; // default to 128KB like C#
    constexpr size_t MAX_ROW_BUFFER_SIZE = 1024 * 1024;
    constexpr size_t TYPICAL_BATCH_SIZE = 32;
    constexpr size_t AUTO_INCREMENT_BUFFER_SPACE = 1024;
    
    // FFI result codes
    constexpr int16_t ITER_EXHAUSTED = -1;
    constexpr int16_t ITER_OK = 0;
    constexpr uint16_t ERROR_BUFFER_TOO_SMALL = 11;
    
    /**
     * Auto-Increment Integration System
     * 
     * This system enables automatic integration of generated auto-increment values
     * back into user row objects after insert operations. The system uses function
     * pointers registered during module initialization to handle the integration
     * per struct type.
     */
    
    /** Function pointer type for auto-increment integration callbacks */
    template<typename T>
    using AutoIncIntegratorFn = void(*)(T&, SpacetimeDB::bsatn::Reader&);
    
    /** Registry to store auto-increment integrators per type */
    template<typename T>
    inline AutoIncIntegratorFn<T>& get_autoinc_integrator() {
        static AutoIncIntegratorFn<T> integrator = nullptr;
        return integrator;
    }
    
    /**
     * Integrate auto-increment values into a row object.
     * 
     * This function is called automatically by Table::insert() when SpacetimeDB
     * returns generated auto-increment values. It looks up the registered integrator
     * for the struct type and calls it to update the row with generated values.
     * 
     * @param row The row object to update with generated values
     * @param reader BSATN reader containing the generated column values
     */
    template<typename T>
    void integrate_autoinc(T& row, SpacetimeDB::bsatn::Reader& reader) {
        auto integrator = get_autoinc_integrator<T>();
        if (integrator) {
            integrator(row, reader);
        }
        // If no integrator registered, do nothing (no auto-increment fields)
    }
    
    // Error handling utilities
    inline std::string format_error(const std::string& context, 
                                   const std::string& operation, 
                                   int code) {
        return context + ": " + operation + " failed with code " + std::to_string(code);
    }
    
    inline void check_buffer_size(size_t size) {
        if (size > MAX_ROW_BUFFER_SIZE) {
            LOG_FATAL("Buffer size exceeds maximum limit");
        }
    }
    
    // Generic error handler for FFI operations
    template<typename Op>
    inline void handle_ffi_error(Status status, 
                                [[maybe_unused]] const std::string& context, 
                                [[maybe_unused]] Op operation) {
        if (is_error(status)) {
            LOG_FATAL("FFI operation failed: " + context);
        }
    }
}

// =============================================================================
// TableIterator - Efficient batch iteration
// =============================================================================

template<typename T>
class TableIterator {
    static_assert(std::is_same_v<T, std::remove_cv_t<T>>, 
                  "TableIterator requires non-const, non-volatile type");

public:
    // STL iterator type definitions
    using iterator_category = std::input_iterator_tag;
    using value_type = T;
    using difference_type = std::ptrdiff_t;
    using pointer = T*;
    using reference = T&;

    // Constructors
    TableIterator() noexcept : iter_handle_(Invalid::ROW_ITER), is_end_(true) {}
    
    explicit TableIterator(TableId table_id) {
        Status status = FFI::datastore_table_scan_bsatn(table_id, &iter_handle_);
        detail::handle_ffi_error(status, "TableIterator", "datastore_table_scan_bsatn");
        advance();
    }

    ~TableIterator() noexcept {
        if (iter_handle_ != Invalid::ROW_ITER && !ffi_exhausted_) {
            FFI::row_iter_bsatn_close(iter_handle_);
        }
    }

    // Move-only semantics
    TableIterator(const TableIterator&) = delete;
    TableIterator& operator=(const TableIterator&) = delete;
    
    TableIterator(TableIterator&& other) noexcept : TableIterator() {
        swap(*this, other);
    }
    
    TableIterator& operator=(TableIterator&& other) noexcept {
        if (this != &other) {
            TableIterator temp(std::move(other));
            swap(*this, temp);
        }
        return *this;
    }
    
    friend void swap(TableIterator& a, TableIterator& b) noexcept {
        using std::swap;
        swap(a.iter_handle_, b.iter_handle_);
        swap(a.row_buffer_, b.row_buffer_);
        swap(a.current_batch_, b.current_batch_);
        swap(a.current_index_, b.current_index_);
        swap(a.current_row_, b.current_row_);
        swap(a.is_valid_, b.is_valid_);
        swap(a.is_end_, b.is_end_);
        swap(a.ffi_exhausted_, b.ffi_exhausted_);
    }

    // Iterator operations

    // Returns mutable reference to current row
    T& operator*() {
        if (!is_valid_) LOG_FATAL("Attempted to dereference invalid iterator");
        return current_row_;
    }

    const T& operator*() const {
        if (!is_valid_) LOG_FATAL("Attempted to dereference invalid iterator");
        return current_row_;
    }
    
    const T* operator->() const { return &**this; }
    
    TableIterator& operator++() {
        advance();
        return *this;
    }
    
    bool operator==(const TableIterator& other) const noexcept {
        return is_valid_ == other.is_valid_;
    }
    
    bool operator!=(const TableIterator& other) const noexcept {
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
    bool ffi_exhausted_ = false;  // Track when FFI iterator is exhausted
    
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
            // FFI iterator exhausted and we've consumed all rows
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
            // No rows in batch and FFI is exhausted
            is_end_ = true;
            is_valid_ = false;
        }
    }
    
    void fetch_batch() {
        row_buffer_.resize(detail::INITIAL_ROW_BUFFER_SIZE);
        size_t buffer_len = row_buffer_.size();
        
        int16_t ret = FFI::row_iter_bsatn_advance(iter_handle_, 
                                                  row_buffer_.data(), 
                                                  &buffer_len);
        
        if (ret == detail::ITER_EXHAUSTED) {
            // Iterator is exhausted, but there might be a final batch of data
            ffi_exhausted_ = true;
            if (buffer_len > 0) {
                // Resize buffer to actual data size and deserialize the final batch
                row_buffer_.resize(buffer_len);
                deserialize_batch(buffer_len);
            }
            // Don't set is_end_ here! We may have multiple rows in this batch
            return;
        }
        
        if (ret == detail::ERROR_BUFFER_TOO_SMALL) {
            detail::check_buffer_size(buffer_len);
            row_buffer_.resize(buffer_len);
            ret = FFI::row_iter_bsatn_advance(iter_handle_, 
                                             row_buffer_.data(), 
                                             &buffer_len);
        }
        
        if (ret > 0) {
            LOG_FATAL("TableIterator::advance failed with error: " + std::to_string(ret));
        }
        
        // CRITICAL: Resize buffer to actual data size before deserializing!
        row_buffer_.resize(buffer_len);
        deserialize_batch(buffer_len);
    }
    
    void deserialize_batch(size_t buffer_len) {
        current_batch_.clear();
        current_batch_.reserve(detail::TYPICAL_BATCH_SIZE);
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
// Table - Type-safe table interface
// =============================================================================

template<typename T>
class Table {
    static_assert(std::is_same_v<T, std::remove_cv_t<T>>, 
                  "Table requires non-const, non-volatile row type");

public:
    explicit Table(TableId table_id) noexcept : table_id_(table_id) {}

    // -------------------------------------------------------------------------
    // Core CRUD Operations
    // -------------------------------------------------------------------------
    
    /**
     * Insert a row and return it with auto-generated fields populated.
     * 
     * For tables with auto-increment fields (defined with FIELD_PrimaryKeyAutoInc, 
     * FIELD_UniqueAutoInc, FIELD_IndexAutoInc, or FIELD_AutoInc), this method 
     * automatically integrates the generated values back into the returned row.
     * 
     * @param row_data The row to insert. Auto-increment fields can be set to 0 
     *                 or any placeholder value - they will be overwritten.
     * @return The inserted row with all auto-increment fields populated with 
     *         their generated values.
     * 
     * @example
     * // Table with auto-increment ID
     * SPACETIMEDB_TABLE(User, users, Public);
     * FIELD_PrimaryKeyAutoInc(users, id);
     * 
     * // In a reducer
     * User user{0, "Alice", true};  // id=0 is placeholder
     * User inserted = ctx.db[users].insert(user);
     * LOG_INFO("Created user with ID: " + std::to_string(inserted.id));
     * 
     * @note The integration system uses registry-based callbacks registered during
     *       module initialization (__preinit__19_) to handle the auto-increment 
     *       value integration automatically.
     */
    T insert(const T& row_data) {
        auto result = try_insert(row_data);
        if (result.is_err()) {
            const auto& error = result.err();
            // Convert to LOG_FATAL to maintain current behavior
            LOG_FATAL("Table::insert failed: " + error.message);
        }
        return result.take_ok();
    }
    
    /**
     * Insert a row and return Result-like type instead of throwing on error.
     * 
     * This method provides the same functionality as insert() but returns
     * an InsertResult<T> that contains either the successfully inserted row
     * (with auto-generated fields populated) or an InsertError with details
     * about what went wrong.
     * 
     * @param row_data The row to insert
     * @return InsertResult<T> containing either the inserted row or error details
     * 
     * @example
     * auto result = ctx.db[users].try_insert(User{0, "Alice", true});
     * if (result.is_ok()) {
     *     const auto& user = result.ok();
     *     LOG_INFO("Created user with ID: " + std::to_string(user.id));
     * } else {
     *     const auto& error = result.err();
     *     LOG_INFO("Insert failed: " + error.message);
     * }
     */
    InsertResult<T> try_insert(const T& row_data) {
        SpacetimeDB::bsatn::Writer writer;
        SpacetimeDB::bsatn::serialize(writer, row_data);
        auto buffer_vec = writer.get_buffer();
        
        // Prepare buffer with extra space for auto-increment writeback
        const size_t original_len = buffer_vec.size();
        const size_t extra_space = detail::AUTO_INCREMENT_BUFFER_SPACE;
        std::vector<uint8_t> buffer(buffer_vec.begin(), buffer_vec.end());
        buffer.resize(original_len + extra_space);
        
        size_t buffer_len = original_len;
        Status status = ::datastore_insert_bsatn(table_id_, buffer.data(), &buffer_len);
        
        // Instead of calling detail::handle_ffi_error (which LOG_FATALs), 
        // handle errors and return appropriate InsertError
        if (is_error(status)) {
            InsertErrorType error_type;
            std::string message;
            
            // Map status codes to our error types
            if (status == StatusCode::UNIQUE_ALREADY_EXISTS) {
                error_type = InsertErrorType::UniqueConstraintViolation;
                message = "Unique constraint violation";
            } else if (status == StatusCode::AUTO_INC_OVERFLOW) {
                error_type = InsertErrorType::AutoIncOverflow;
                message = "Auto increment overflow";
            } else {
                error_type = InsertErrorType::Other;
                message = "Insert failed with status: " + std::string(StatusCode::to_string(status));
            }
            
            return InsertResult<T>(InsertError(error_type, status, message));
        }
        
        // Success path - same as current insert()
        if (buffer_len == 0) {
            // No auto-generated fields, return the original row
            return InsertResult<T>(T(row_data));
        }
        
        // The buffer contains ONLY the generated column values in BSATN format
        T updated_row = row_data;
        SpacetimeDB::bsatn::Reader reader(buffer.data(), buffer_len);
        detail::integrate_autoinc(updated_row, reader);
        
        return InsertResult<T>(std::move(updated_row));
    }
   

    /**
     * Delete all rows matching the given values
     */
    uint32_t delete_all_by_eq(const std::vector<T>& rows) {
        if (rows.empty()) return 0;
        
        SpacetimeDB::bsatn::Writer writer;
        writer.write_u32_le(static_cast<uint32_t>(rows.size()));
        for (const auto& row : rows) {
            SpacetimeDB::bsatn::serialize(writer, row);
        }
        
        auto buffer = writer.take_buffer();
        uint32_t deleted = 0;
        Status status = FFI::datastore_delete_all_by_eq_bsatn(
            table_id_, buffer.data(), buffer.size(), &deleted);
        detail::handle_ffi_error(status, "Table::delete_all_by_eq", 
                               "datastore_delete_all_by_eq_bsatn");
        
        return deleted;
    }
    
    /**
     * Delete a single row by value
     */
    bool delete_by_value(const T& row) {
        return delete_all_by_eq({row}) > 0;
    }
    
    /**
     * Update a row using a unique index
     */
    std::optional<T> update_by_index(IndexId index_id, const T& row) {
        SpacetimeDB::bsatn::Writer writer;
        SpacetimeDB::bsatn::serialize(writer, row);
        auto buffer_vec = writer.get_buffer();
        
        // Prepare buffer with extra space for auto-increment writeback
        const size_t original_len = buffer_vec.size();
        const size_t extra_space = detail::AUTO_INCREMENT_BUFFER_SPACE;
        std::vector<uint8_t> buffer(buffer_vec.begin(), buffer_vec.end());
        buffer.resize(original_len + extra_space);
        
        size_t buffer_len = original_len;
        Status status = FFI::datastore_update_bsatn(
            table_id_, index_id, buffer.data(), &buffer_len);
        
        if (status == StatusCode::NO_SUCH_ROW) {
            return std::nullopt;
        }
        
        if (status == StatusCode::INDEX_NOT_UNIQUE) {
            LOG_FATAL("Update failed: index is not unique");
        }
        
        if (status == StatusCode::NO_SUCH_INDEX) {
            LOG_FATAL("Update failed: index does not exist");
        }
        
        detail::handle_ffi_error(status, "Table::update_by_index", 
                               "datastore_update_bsatn");
        
        // Handle the case where buffer_len might be 0 (no auto-increment fields)
        if (buffer_len == 0) {
            // No auto-generated fields, return the original row
            return row;
        }
        
        // Return the updated row with any auto-generated fields
        buffer.resize(buffer_len);
        SpacetimeDB::bsatn::Reader reader(buffer.data(), buffer_len);
        return SpacetimeDB::bsatn::deserialize<T>(reader);
    }

    // -------------------------------------------------------------------------
    // Iteration Support
    // -------------------------------------------------------------------------
    
    TableIterator<T> begin() { return TableIterator<T>(table_id_); }
    TableIterator<T> end() { return TableIterator<T>(); }

    // -------------------------------------------------------------------------
    // Table Metadata
    // -------------------------------------------------------------------------
    
    uint64_t count() {
        uint64_t result = 0;
        Status status = FFI::datastore_table_row_count(table_id_, &result);
        detail::handle_ffi_error(status, "Table::count", "datastore_table_row_count");
        return result;
    }
    
    bool empty() { return count() == 0; }
    
    TableId get_table_id() const noexcept { return table_id_; }

private:
    TableId table_id_;
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_LIBRARY_TABLE_H