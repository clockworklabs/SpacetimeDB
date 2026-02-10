#ifndef SPACETIMEDB_READONLY_DATABASE_CONTEXT_H
#define SPACETIMEDB_READONLY_DATABASE_CONTEXT_H

#include "readonly_table_accessor.h"
#include "readonly_field_accessors.h"
#include "database.h"
#include <string>

namespace SpacetimeDB {

/**
 * @brief Read-only database context for views
 * 
 * ReadOnlyDatabaseContext provides a read-only interface to the database
 * for use in views. It prevents all mutation operations at compile-time.
 * 
 * Key differences from DatabaseContext:
 * - No insert/update/delete operations
 * - No direct table iteration (prevents inefficient full table scans)
 * - Table data accessible ONLY through indexed field accessors
 * - Enforces efficient query patterns using indexes
 * 
 * This is a completely separate type from DatabaseContext (no inheritance)
 * to match Rust's LocalReadOnly vs Local pattern.
 * 
 * Example usage:
 * @code
 * SPACETIMEDB_VIEW(std::vector<User>, get_adults, Public, ViewContext ctx) {
 *     // Can only access via indexed fields
 *     std::vector<User> adults;
 *     for (const auto& user : ctx.db[user_age].filter(range_from(18u))) {
 *         adults.push_back(user);
 *     }
 *     return Ok(adults);
 * }
 * @endcode
 */
class ReadOnlyDatabaseContext {
public:
    // Generic table accessor method (type-only, requires explicit table name later)
    template<typename T>
    ReadOnlyTableAccessor<T> table() const {
        return ReadOnlyTableAccessor<T>{};
    }
    
    // Name-based accessor that returns a configured table accessor
    template<typename T>
    ReadOnlyTableAccessor<T> table(const char* name) const {
        return ReadOnlyTableAccessor<T>(std::string(name));
    }
    
    // String overload
    template<typename T>
    ReadOnlyTableAccessor<T> table(const std::string& name) const {
        return table<T>(name.c_str());
    }
    
    // Tag-based accessor using operator[] (SpacetimeDB standard)
    template<typename Tag>
    ReadOnlyTableAccessor<typename Tag::type> operator[](const Tag&) const {
        return ReadOnlyTableAccessor<typename Tag::type>(std::string(Tag::__table_name_internal));
    }
    
    // Field tag accessors - read-only versions
    // These return read-only field accessors that only support querying, not mutation
    
    template<typename TableType, typename FieldType>
    ReadOnlyPrimaryKeyAccessor<TableType, FieldType> operator[](
        const FieldTag<TableType, FieldType, FieldConstraint::PrimaryKey>& field_tag) const {
        return ReadOnlyPrimaryKeyAccessor<TableType, FieldType>(
            field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
    
    template<typename TableType, typename FieldType>
    ReadOnlyUniqueAccessor<TableType, FieldType> operator[](
        const FieldTag<TableType, FieldType, FieldConstraint::Unique>& field_tag) const {
        return ReadOnlyUniqueAccessor<TableType, FieldType>(
            field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
    
    template<typename TableType, typename FieldType>
    ReadOnlyIndexedAccessor<TableType, FieldType> operator[](
        const FieldTag<TableType, FieldType, FieldConstraint::Indexed>& field_tag) const {
        return ReadOnlyIndexedAccessor<TableType, FieldType>(
            field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
    
    template<typename TableType, typename FieldType>
    ReadOnlyRegularAccessor<TableType, FieldType> operator[](
        const FieldTag<TableType, FieldType, FieldConstraint::None>& field_tag) const {
        return ReadOnlyRegularAccessor<TableType, FieldType>(
            field_tag.table_name, field_tag.field_name, field_tag.member_ptr);
    }
};

} // namespace SpacetimeDB

#endif // SPACETIMEDB_READONLY_DATABASE_CONTEXT_H
