#ifndef SPACETIMEDB_READONLY_TABLE_ACCESSOR_H
#define SPACETIMEDB_READONLY_TABLE_ACCESSOR_H

#include "table.h"
#include "abi/FFI.h"
#include "logger.h"
#include <string>
#include <optional>
#include <type_traits>

namespace SpacetimeDB {

/**
 * @brief Read-only table accessor for views
 * 
 * ReadOnlyTableAccessor provides compile-time read-only access to tables.
 * Unlike TableAccessor, it:
 * - Deletes all mutation operations (insert/update/delete)
 * - Deletes direct iteration methods (begin/end/collect)
 * - Forces use of indexed field accessors for efficient queries
 * 
 * This design enforces two critical properties for views:
 * 1. Read-only access (no mutations)
 * 2. Efficient queries (no full table scans)
 * 
 * Table data is ONLY accessible through indexed field accessors:
 * - ctx.db[table_field].filter(range) for indexed fields
 * - ctx.db[table_field].find(key) for unique/primary key fields
 * - ctx.db[table].count() for counting (doesn't require iteration)
 * 
 * Example usage:
 * @code
 * // ✅ Allowed - query via indexed field
 * for (const auto& person : ctx.db[person_age].filter(range_from(18u))) {
 *     process(person);
 * }
 * 
 * // ✅ Allowed - count doesn't require iteration
 * uint64_t total = ctx.db[person].count();
 * 
 * // ❌ Compile error - no direct iteration
 * for (const auto& person : ctx.db[person]) { }  // DELETED
 * 
 * // ❌ Compile error - no collect()
 * auto all = ctx.db[person].collect();  // DELETED
 * 
 * // ❌ Compile error - no mutations
 * ctx.db[person].insert(new_person);  // DELETED
 * @endcode
 */
template<typename T>
class ReadOnlyTableAccessor {
protected:
    mutable std::optional<TableId> table_id_;
    std::string table_name_;
    
    TableId resolve_table_id() const {
        if (!table_id_.has_value()) {
            std::string name_to_use = table_name_;
            
            if (name_to_use.empty()) {
                LOG_FATAL("Table name is required");
            }
            
            TableId id;
            Status status = ::table_id_from_name(
                reinterpret_cast<const uint8_t*>(name_to_use.c_str()),
                name_to_use.length(),
                &id
            );
            
            if (SpacetimeDB::is_error(status)) {
                LOG_FATAL("Table not found: " + name_to_use);
            }
            table_id_ = id;
        }
        return *table_id_;
    }
    
public:
    // Constructor that accepts a table name
    ReadOnlyTableAccessor() = default;
    explicit ReadOnlyTableAccessor(const std::string& table_name) : table_name_(table_name) {}
    
    // ✅ ALLOWED: Count rows (doesn't require iteration)
    uint64_t count() const {
        TableId table_id = resolve_table_id();
        uint64_t out_count;
        Status status = ::datastore_table_row_count(table_id, &out_count);
        if (SpacetimeDB::is_error(status)) {
            LOG_FATAL("Failed to count rows in table: " + table_name_);
        }
        return out_count;
    }
    
    // ❌ DELETED: No direct iteration - prevents inefficient full table scans
    // Views must use indexed field accessors: ctx.db[table_field].filter(range)
    auto begin() const = delete;
    auto end() const = delete;
    
    // ❌ DELETED: No collect() - prevents inefficient full table scans
    // Views must use indexed field accessors to retrieve data
    std::vector<T> collect() const = delete;
    
    // ❌ DELETED: No table() method - would bypass read-only protection
    SpacetimeDB::Table<T> table() const = delete;
    SpacetimeDB::Table<T> get_table() const = delete;
    
    // ❌ DELETED: No mutations allowed in views
    T insert(const T& row) const = delete;
    uint32_t delete_by_value(const T& value) const = delete;
    uint32_t update_by_value(const T& old_value, const T& new_value) const = delete;
};

} // namespace SpacetimeDB

// Use spacetimedb namespace for consistency
namespace spacetimedb {
    template<typename T>
    using ReadOnlyTableAccessor = SpacetimeDB::ReadOnlyTableAccessor<T>;
}

#endif // SPACETIMEDB_READONLY_TABLE_ACCESSOR_H
