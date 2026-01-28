#ifndef SPACETIMEDB_DATABASE_H
#define SPACETIMEDB_DATABASE_H

#include "table.h"
// Removed: internal/field_metadata.h (consolidated into field_registration.h)  
#include "abi/FFI.h"
#include "bsatn/bsatn.h"
#include "bsatn/traits.h"
#include "logger.h"
#include <type_traits>
#include <string>
#include <stdexcept>
#include <string_view>
#include <optional>
#include <vector>

// Include field-based accessor system (SpacetimeDB standard pattern)
// #include "field_accessors.h" // Removed - field accessors are now in table_with_constraints.h

// Forward declarations to avoid circular includes
namespace SpacetimeDB {
namespace Internal {
    class Module;
    struct RawModuleDef;
}

// Field constraint flags - must match Rust's ColumnAttribute bits  
enum class FieldConstraint : uint32_t {
    None = 0,
    Indexed = 0b0001,                         // 1
    AutoInc = 0b0010,                         // 2
    Unique = Indexed | 0b0100,                // 5 (Indexed + Unique bit)
    PrimaryKey = Unique | 0b1000,             // 13 (Unique + PrimaryKey bit)
    Identity = Unique | AutoInc,              // 7 (Unique + AutoInc)
    PrimaryKeyAuto = PrimaryKey | AutoInc,    // 15 (PrimaryKey + AutoInc)
    NotNull = 1 << 4                          // 16 (not used in Rust but kept for future)
};

inline FieldConstraint operator|(FieldConstraint a, FieldConstraint b) {
    return static_cast<FieldConstraint>(
        static_cast<uint32_t>(a) | static_cast<uint32_t>(b)
    );
}

constexpr bool has_constraint(FieldConstraint field, FieldConstraint constraint) {
    return (static_cast<uint32_t>(field) & static_cast<uint32_t>(constraint)) != 0;
}

// Forward declaration for tag-based accessors
template<typename T>
struct TableTag;


// Forward declaration for field tags
template<typename TableType, typename FieldType, FieldConstraint Constraint>
struct FieldTag;

// Forward declarations for typed field accessors
template<typename TableType, typename FieldType>
class TypedPrimaryKeyAccessor;

template<typename TableType, typename FieldType>
class TypedUniqueAccessor;

template<typename TableType, typename FieldType>
class TypedIndexedAccessor;

template<typename TableType, typename FieldType>
class TypedRegularAccessor;

// Forward declaration for multi-column index support
template<typename TableType>
struct MultiColumnIndexTag;

template<typename TableType>
class TypedMultiColumnIndexAccessor;

// Constraint system definitions for primary key operations

// Field constraint info structure
struct FieldConstraintInfo {
    const char* field_name;
    FieldConstraint constraints;
    const char* index_name = nullptr;  // For named indexes
    std::vector<const char*> column_names;  // For multi-column indexes
    
    // Constructor for basic constraints
    FieldConstraintInfo(const char* name, FieldConstraint c) 
        : field_name(name), constraints(c), index_name(nullptr) {}
    
    // Constructor for named indexes
    FieldConstraintInfo(const char* name, FieldConstraint c, const char* idx_name) 
        : field_name(name), constraints(c), index_name(idx_name) {}
    
    // Constructor for multi-column indexes
    FieldConstraintInfo(std::initializer_list<const char*> columns, FieldConstraint c, const char* idx_name)
        : field_name(nullptr), constraints(c), index_name(idx_name), column_names(columns) {}
};

// Table accessor that resolves table names at runtime with constraint-aware operations
template<typename T>
class TableAccessor {
protected:
    mutable std::optional<TableId> table_id_;
    std::string table_name_;
    
    TableId resolve_table_id() const {
        if (!table_id_.has_value()) {
            // Resolve table ID from name
            
            // Use the provided table name or lookup from module metadata
            std::string name_to_use = table_name_;
            
            if (name_to_use.empty()) {
                // For now, require explicit table names
                // TODO: Add runtime table name lookup
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
    TableAccessor() = default;
    explicit TableAccessor(const std::string& table_name) : table_name_(table_name) {}
    
    // Insert a row and return it with any auto-generated fields
    T insert(const T& row) const {
        return get_table().insert(row);
    }
    
    // Count all rows in the table
    uint64_t count() const {
        return get_table().count();
    }
    
    // Delete rows matching a value
    uint32_t delete_by_value(const T& value) const {
        return get_table().delete_by_value(value);
    }
    
    // Internal helper used by typed field accessors for atomic updates
    uint32_t update_by_value(const T& old_value, const T& new_value) const {
        // Delegate to Table<T> for atomic delete + insert
        uint32_t deleted_count = get_table().delete_by_value(old_value);
        if (deleted_count > 0) {
            for (uint32_t i = 0; i < deleted_count; ++i) {
                get_table().insert(new_value);
            }
        }
        return deleted_count;
    }

public:
    // Get or create cached Table<T> instance
    SpacetimeDB::Table<T> get_table() const {
        return SpacetimeDB::Table<T>(resolve_table_id());
    }
    
    // Iterate over all rows in the table
    SpacetimeDB::Table<T> table() const {
        return get_table();
    }
    
    // Range-based for loop support
    auto begin() const { return get_table().begin(); }
    auto end() const { return get_table().end(); }
};

/**
 * @brief Database context with name-based table accessors (RECOMMENDED API)
 * 
 * DatabaseContext provides the recommended interface for all table operations
 * in C++ modules. It automatically handles table ID resolution and provides
 * a reliable wrapper around the low-level Table API.
 * 
 * @note This is the RECOMMENDED way to perform table operations. Direct Table
 *       construction has known issues with insert operations that can cause crashes.
 * 
 * Example usage:
 * @code
 * SPACETIMEDB_REDUCER(my_reducer, ReducerContext ctx, uint32_t id, std::string name)
 * {
 *     // ALWAYS use DatabaseContext through ctx.db
 *     auto users = ctx.db.table<User>("users");
 *     
 *     // All operations work reliably
 *     User new_user = users.insert({id, name, 30});
 *     uint64_t count = users.count();
 *     
 *     for (const auto& user : users) {
 *         LOG_INFO_F("User: %s", user.name.c_str());
 *     }
 * }
 * @endcode
 */
// Database context with name-based table accessors
class DatabaseContext {
public:
    // Generic table accessor method (type-only, requires explicit table name later)
    template<typename T>
    TableAccessor<T> table() const {
        return TableAccessor<T>{};
    }
    
    // Name-based accessor that returns a configured table accessor
    template<typename T>
    TableAccessor<T> table(const char* name) const {
        return TableAccessor<T>(std::string(name));
    }
    
    // String overload
    template<typename T>
    TableAccessor<T> table(const std::string& name) const {
        return table<T>(name.c_str());
    }
    
    // Tag-based accessor using operator[] (SpacetimeDB standard)
    template<typename Tag>
    TableAccessor<typename Tag::type> operator[](const Tag&) const {
        return TableAccessor<typename Tag::type>(std::string(Tag::__table_name_internal));
    }
    
    // Field tag accessor - NEW: ctx.db[simple_table.id] syntax
    // Overloaded for each field constraint type
    template<typename TableType, typename FieldType>
    TypedPrimaryKeyAccessor<TableType, FieldType> operator[](const FieldTag<TableType, FieldType, FieldConstraint::PrimaryKey>& field_tag) const {
        return TypedPrimaryKeyAccessor<TableType, FieldType>(field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
    
    template<typename TableType, typename FieldType>
    TypedUniqueAccessor<TableType, FieldType> operator[](const FieldTag<TableType, FieldType, FieldConstraint::Unique>& field_tag) const {
        return TypedUniqueAccessor<TableType, FieldType>(field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
    
    template<typename TableType, typename FieldType>
    TypedIndexedAccessor<TableType, FieldType> operator[](const FieldTag<TableType, FieldType, FieldConstraint::Indexed>& field_tag) const {
        return TypedIndexedAccessor<TableType, FieldType>(field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
    
    template<typename TableType, typename FieldType>
    TypedRegularAccessor<TableType, FieldType> operator[](const FieldTag<TableType, FieldType, FieldConstraint::None>& field_tag) const {
        return TypedRegularAccessor<TableType, FieldType>(field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
    
    // Multi-column index accessor - NEW: ctx.db[score.by_player_and_level] syntax
    template<typename TableType>
    TypedMultiColumnIndexAccessor<TableType> operator[](const MultiColumnIndexTag<TableType>& index_tag) const {
        return TypedMultiColumnIndexAccessor<TableType>(index_tag.table_name, index_tag.index_name, index_tag.column_list);
    }
};


} // namespace SpacetimeDB

// Use spacetimedb namespace for consistency
namespace spacetimedb {
    template<typename T>
    using TableAccessor = SpacetimeDB::TableAccessor<T>;
}

#endif // SPACETIMEDB_DATABASE_H