#ifndef SPACETIMEDB_READONLY_FIELD_ACCESSORS_H
#define SPACETIMEDB_READONLY_FIELD_ACCESSORS_H

#include "table.h"
#include "abi/FFI.h"
#include "index_iterator.h"
#include "range_queries.h"
#include "logger.h"
#include <string>
#include <vector>
#include <optional>
#include <type_traits>

namespace SpacetimeDB {

// Forward declare to avoid circular dependency
namespace detail {
    std::vector<std::string> get_index_patterns(const std::string& table_name, 
                                                const std::string& field_name,
                                                FieldConstraint constraint_type);
}

/**
 * @brief Base class for read-only field accessors
 * 
 * Provides common functionality for all read-only field accessors.
 * Read-only accessors only support querying operations, no mutations.
 */
template<typename TableType, typename FieldType>
class ReadOnlyFieldAccessorBase {
protected:
    const char* table_name_;
    const char* field_name_;
    FieldType TableType::*member_ptr_;
    mutable std::optional<TableId> table_id_;
    mutable std::optional<IndexId> index_id_;
    
    TableId resolve_table_id() const {
        if (!table_id_.has_value()) {
            TableId id;
            Status status = ::table_id_from_name(
                reinterpret_cast<const uint8_t*>(table_name_),
                std::strlen(table_name_),
                &id
            );
            if (SpacetimeDB::is_error(status)) {
                LOG_FATAL(std::string("Table not found: ") + table_name_);
            }
            table_id_ = id;
        }
        return *table_id_;
    }
    
    IndexId resolve_index_with_patterns(const std::vector<std::string>& patterns) const {
        if (index_id_.has_value()) {
            return *index_id_;
        }
        
        for (const auto& pattern : patterns) {
            IndexId id;
            Status status = ::index_id_from_name(
                reinterpret_cast<const uint8_t*>(pattern.c_str()),
                pattern.length(),
                &id
            );
            if (is_ok(status)) {
                index_id_ = id;
                return id;
            }
        }
        return IndexId{0};
    }
    
    [[nodiscard]] virtual IndexId get_index_id() const = 0;
    
    FieldType get_field_value(const TableType& row) const {
        return row.*member_ptr_;
    }
    
public:
    ReadOnlyFieldAccessorBase(const char* table_name, const char* field_name, 
                              FieldType TableType::*member_ptr)
        : table_name_(table_name), field_name_(field_name), member_ptr_(member_ptr) {}
    
    virtual ~ReadOnlyFieldAccessorBase() = default;
    
    // ❌ DELETED: No mutations allowed in views
    bool insert(const TableType& row) const = delete;
    bool delete_by_value(const FieldType& value) const = delete;
    bool update(const TableType& new_row) const = delete;
    uint32_t delete_all(const FieldType& value) const = delete;
};

/**
 * @brief Read-only accessor for primary key fields
 * 
 * Allows only:
 * - find(key) -> std::optional<TableType>
 */
template<typename TableType, typename FieldType>
class ReadOnlyPrimaryKeyAccessor : public ReadOnlyFieldAccessorBase<TableType, FieldType> {
private:
    [[nodiscard]] IndexId get_index_id() const override {
        auto patterns = detail::get_index_patterns(
            std::string(this->table_name_), 
            std::string(this->field_name_),
            FieldConstraint::PrimaryKey
        );
        IndexId id = this->resolve_index_with_patterns({patterns[0], patterns[1], patterns[2]});
        if (id.inner == 0) {
            LOG_FATAL(std::string("Failed to resolve index for primary key field: ") + 
                     this->table_name_ + "." + this->field_name_);
        }
        return id;
    }
    
public:
    using ReadOnlyFieldAccessorBase<TableType, FieldType>::ReadOnlyFieldAccessorBase;
    
    /**
     * Find a single row by primary key value
     * Returns std::nullopt if not found
     */
    std::optional<TableType> find(const FieldType& value) const {
        IndexId index_id = get_index_id();
        if (index_id.inner != 0) {
            IndexIterator<TableType> iter(index_id, value);
            if (iter != IndexIterator<TableType>()) {
                return *iter;
            }
        }
        return std::nullopt;
    }
    
    /**
     * Try to get a row by primary key value
     * Alias for find() for consistency with writable accessor
     */
    std::optional<TableType> try_get(const FieldType& value) const {
        return find(value);
    }
};

/**
 * @brief Read-only accessor for unique fields
 * 
 * Allows only:
 * - find(key) -> std::optional<TableType>
 */
template<typename TableType, typename FieldType>
class ReadOnlyUniqueAccessor : public ReadOnlyFieldAccessorBase<TableType, FieldType> {
private:
    [[nodiscard]] IndexId get_index_id() const override {
        auto patterns = detail::get_index_patterns(
            std::string(this->table_name_), 
            std::string(this->field_name_),
            FieldConstraint::Unique
        );
        IndexId id = this->resolve_index_with_patterns({patterns[0], patterns[1], patterns[2]});
        if (id.inner == 0) {
            LOG_FATAL(std::string("Failed to resolve index for unique field: ") + 
                     this->table_name_ + "." + this->field_name_);
        }
        return id;
    }
    
public:
    using ReadOnlyFieldAccessorBase<TableType, FieldType>::ReadOnlyFieldAccessorBase;
    
    /**
     * Find a single row by unique field value
     * Returns std::nullopt if not found
     */
    std::optional<TableType> find(const FieldType& value) const {
        IndexId index_id = get_index_id();
        if (index_id.inner != 0) {
            IndexIterator<TableType> iter(index_id, value);
            if (iter != IndexIterator<TableType>()) {
                return *iter;
            }
        }
        return std::nullopt;
    }
};

/**
 * @brief Read-only accessor for indexed (non-unique) fields
 * 
 * Allows only:
 * - filter(value) -> lazy IndexIterator over matching rows
 * - filter(range) -> lazy IndexIterator over range-matched rows
 * 
 * IndexIterator supports both traditional and range-based for loops.
 * Results are evaluated lazily without materializing all matches.
 * Call .collect() to materialize results into a std::vector if needed.
 */
template<typename TableType, typename FieldType>
class ReadOnlyIndexedAccessor : public ReadOnlyFieldAccessorBase<TableType, FieldType> {
private:
    [[nodiscard]] IndexId get_index_id() const override {
        auto patterns = detail::get_index_patterns(
            std::string(this->table_name_), 
            std::string(this->field_name_),
            FieldConstraint::Indexed
        );
        if (patterns.size() >= 2) {
            return this->resolve_index_with_patterns({patterns[0], patterns[1]});
        } else if (patterns.size() == 1) {
            return this->resolve_index_with_patterns({patterns[0]});
        }
        LOG_FATAL(std::string("Failed to resolve index for indexed field: ") + 
                 this->table_name_ + "." + this->field_name_);
        return IndexId{0};
    }
    
public:
    using ReadOnlyFieldAccessorBase<TableType, FieldType>::ReadOnlyFieldAccessorBase;
    
    /**
     * Filter rows by exact field value using index
     * Returns lazy IndexIterator - results evaluated during iteration
     * Always use via range-based for loop for clean syntax
     * 
     * Example:
     * for (const auto& person : ctx.db[person_age].filter(25u)) {
     *     // process persons aged 25 - no materialization overhead
     * }
     * 
     * To materialize all matching rows into a vector:
     * auto all_aged_25 = ctx.db[person_age].filter(25u).collect();
     */
    IndexIteratorRange<TableType> filter(const FieldType& value) const {
        IndexId index_id = get_index_id();
        if (index_id.inner != 0) {
            return IndexIteratorRange<TableType>(IndexIterator<TableType>(index_id, value));
        }
        return IndexIteratorRange<TableType>(IndexIterator<TableType>());
    }
    
    /**
     * Filter rows by range using index
     * Returns lazy IndexIterator - results evaluated during iteration
     * Always use via range-based for loop for clean syntax
     * 
     * Example:
     * for (const auto& person : ctx.db[person_age].filter(range_from(18u))) {
     *     // process persons aged 18+ - no materialization overhead
     * }
     */
    template<typename RangeType>
    std::enable_if_t<is_range_v<RangeType>, IndexIteratorRange<TableType>>
    filter(const RangeType& range) const {
        IndexId index_id = get_index_id();
        if (index_id.inner != 0) {
            return IndexIteratorRange<TableType>(IndexIterator<TableType>(index_id, range));
        }
        return IndexIteratorRange<TableType>(IndexIterator<TableType>());
    }
};

/**
 * @brief Read-only accessor for regular (non-indexed) fields
 * 
 * Regular fields have no index, so they cannot be queried efficiently in views.
 * All methods are deleted to enforce this at compile-time.
 */
template<typename TableType, typename FieldType>
class ReadOnlyRegularAccessor {
private:
    const char* table_name_;
    const char* field_name_;
    FieldType TableType::*member_ptr_;
    
public:
    ReadOnlyRegularAccessor(const char* table_name, const char* field_name, 
                           FieldType TableType::*member_ptr)
        : table_name_(table_name), field_name_(field_name), member_ptr_(member_ptr) {}
    
    // ❌ DELETED: Regular fields have no index, cannot be queried in views
    // Views must use indexed, unique, or primary key fields for queries
    auto filter(const FieldType& value) const = delete;
    std::optional<TableType> find(const FieldType& value) const = delete;
    
    // ❌ DELETED: No mutations allowed in views
    bool insert(const TableType& row) const = delete;
    bool delete_by_value(const FieldType& value) const = delete;
    bool update(const TableType& new_row) const = delete;
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_READONLY_FIELD_ACCESSORS_H
