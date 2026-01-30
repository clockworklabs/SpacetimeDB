#pragma once

#include "internal/Module.h"
#include "internal/field_registration.h"
#include "internal/autogen/RawScheduleDefV9.g.h"
#include "macros.h"
#include "error_handling.h" // For DatabaseResult, DatabaseError, UpsertResult
#include "index_iterator.h" // For IndexIterator
#include "range_queries.h"  // For Range types and is_range_v
#include <string>
#include <vector>
#include <cstdint>
#include <cstring>
#include <optional>
#include <type_traits>
#include <concepts>

namespace SpacetimeDB {

// =============================================================================
// Helper Functions
// =============================================================================

namespace detail {
    // Common index name patterns for different constraint types
    inline std::vector<std::string> get_index_patterns(const std::string& table_name, 
                                                       const std::string& field_name,
                                                       FieldConstraint constraint_type) {
        // Check for primary key (with or without auto-increment)
        if (has_constraint(constraint_type, FieldConstraint::PrimaryKey)) {
            return {
                table_name + "_" + field_name + "_idx_btree",
                table_name + "_" + field_name + "_idx",
                "btree_" + table_name + "_" + field_name
            };
        }
        // Check for unique (with or without auto-increment)
        else if (has_constraint(constraint_type, FieldConstraint::Unique)) {
            return {
                table_name + "_" + field_name + "_idx_btree",
                table_name + "_" + field_name + "_unique_idx",
                "btree_" + table_name + "_" + field_name
            };
        }
        // Check for indexed (with or without auto-increment)
        else if (has_constraint(constraint_type, FieldConstraint::Indexed)) {
            return {
                table_name + "_" + field_name + "_idx_btree",  // Database-generated pattern (most likely)
                table_name + "_" + field_name + "_idx",
                "idx_" + table_name + "_" + field_name
            };
        }
        return {};
    }
}

// =============================================================================
// Core Table Tag System
// =============================================================================

/**
 * @brief Base class for table tag types
 * 
 * Each table gets a tag type that acts as an alias for clean syntax:
 * ctx.db[person].insert(...) instead of ctx.db.table<Person>("person")
 */
template<typename T>
struct TableTag {
    using type = T;
    static constexpr const char* name = nullptr;
    
    static std::vector<FieldConstraintInfo> get_constraints() {
        return {};
    }
    
    constexpr TableTag() = default;
};

// =============================================================================
// Table Registration
// =============================================================================

template<typename T>
void register_table_type_with_constraints(const char* name, bool is_public, 
                                        const std::vector<FieldConstraintInfo>& constraints) {
    SpacetimeDB::Internal::Module::RegisterTableInternalImpl<T>(name, is_public, constraints);
}

// =============================================================================
// Main Table Registration Macro
// =============================================================================

/**
 * @brief Register a table with the SpacetimeDB module
 * 
 * Usage: SPACETIMEDB_TABLE(Type, table_name, Public)
 * Creates: Database table named "table_name" and tag variable 'table_name'
 * 
 * Note: Constraints must be added using FIELD_ macros after the table declaration:
 *   SPACETIMEDB_TABLE(User, users, Public)
 *   FIELD_PrimaryKeyAutoInc(users, id)
 *   FIELD_Unique(users, email)
 */
#define SPACETIMEDB_TABLE(type, table_name, access_enum) \
    extern "C" __attribute__((export_name("__preinit__20_register_table_" #type "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__20_register_table_, SPACETIMEDB_PASTE(type, SPACETIMEDB_PASTE(_line_, __LINE__)))() { \
        bool is_public = (access_enum == SpacetimeDB::Internal::TableAccess::Public); \
        SpacetimeDB::Module::RegisterTable<type>(#table_name, is_public); \
    } \
    struct SPACETIMEDB_PASTE(table_name, _tag_type) : SpacetimeDB::TableTag<type> { \
        static constexpr const char* __table_name_internal = #table_name; \
    }; \
    constexpr SPACETIMEDB_PASTE(table_name, _tag_type) table_name{};

/**
 * @brief Schedule a table for automatic reducer execution
 */
#define SPACETIMEDB_SCHEDULE(table_name, scheduled_at_column_index, reducer_name) \
    extern "C" __attribute__((export_name("__preinit__19_schedule_" #table_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__19_schedule_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_line_, __LINE__)))() { \
        SpacetimeDB::Internal::getV9Builder().RegisterSchedule(#table_name, scheduled_at_column_index, #reducer_name); \
    }

// =============================================================================
// Field Tag System
// =============================================================================

template<typename TableType, typename FieldType, SpacetimeDB::FieldConstraint Constraint>
struct FieldTag {
    const char* field_name;
    const char* table_name;
    FieldType TableType::*member_ptr;
    
    static constexpr SpacetimeDB::FieldConstraint constraint = Constraint;
    using table_type = TableType;
    using field_type = FieldType;
    
    constexpr FieldTag(const char* table, const char* field, FieldType TableType::*ptr) 
        : field_name(field), table_name(table), member_ptr(ptr) {}
};

template<typename TableType, typename FieldType>
using PrimaryKeyFieldTag = FieldTag<TableType, FieldType, SpacetimeDB::FieldConstraint::PrimaryKey>;

template<typename TableType, typename FieldType>  
using UniqueFieldTag = FieldTag<TableType, FieldType, SpacetimeDB::FieldConstraint::Unique>;

template<typename TableType, typename FieldType>
using IndexedFieldTag = FieldTag<TableType, FieldType, SpacetimeDB::FieldConstraint::Indexed>;

// =============================================================================
// Multi-Column Index Tag System
// =============================================================================

template<typename TableType>
struct MultiColumnIndexTag {
    const char* table_name;
    const char* index_name;
    const char* column_list;  // List of column names like "player_id_level"
    
    constexpr MultiColumnIndexTag(const char* table, const char* index, const char* columns)
        : table_name(table), index_name(index), column_list(columns) {}
};

// =============================================================================
// Constraint Concepts
// =============================================================================

template<typename T>
concept FilterableValue = 
    std::integral<T> ||
    std::same_as<T, std::string> ||
    std::same_as<T, SpacetimeDB::Identity> ||
    std::same_as<T, SpacetimeDB::ConnectionId> ||
    std::same_as<T, SpacetimeDB::Timestamp> ||
    std::same_as<T, SpacetimeDB::Uuid> ||
    std::same_as<T, SpacetimeDB::I128> ||
    std::same_as<T, SpacetimeDB::U128> ||
    std::same_as<T, SpacetimeDB::I256> ||
    std::same_as<T, SpacetimeDB::U256> ||
    std::same_as<T, SpacetimeDB::i256> ||
    std::same_as<T, SpacetimeDB::u256> ||
    std::is_enum_v<T>;

template<typename T>
concept AutoIncrementable = 
    std::same_as<T, int8_t> ||
    std::same_as<T, int16_t> ||
    std::same_as<T, int32_t> ||
    std::same_as<T, int64_t> ||
    std::same_as<T, uint8_t> ||
    std::same_as<T, uint16_t> ||
    std::same_as<T, uint32_t> ||
    std::same_as<T, uint64_t> ||
    std::same_as<T, SpacetimeDB::I128> ||
    std::same_as<T, SpacetimeDB::U128> ||
    std::same_as<T, SpacetimeDB::i256> ||
    std::same_as<T, SpacetimeDB::u256>;

// =============================================================================
// Unified Field Accessor Base Class
// =============================================================================

template<typename TableType, typename FieldType>
class TypedFieldAccessor : public SpacetimeDB::TableAccessor<TableType> {
protected:
    std::string_view field_name_;
    FieldType TableType::*member_ptr_;
    mutable std::optional<IndexId> cached_index_id_;
    
    [[nodiscard]] constexpr FieldType get_field_value(const TableType& row) const {
        return row.*member_ptr_;
    }
    
    [[nodiscard]] IndexId resolve_index_with_patterns(std::initializer_list<std::string> patterns) const {
        if (cached_index_id_) {
            return *cached_index_id_;
        }
        
        for (const auto& pattern : patterns) {
            IndexId id;
            Status result = ::index_id_from_name(
                reinterpret_cast<const uint8_t*>(pattern.data()),
                pattern.length(),
                &id
            );
            
            if (is_ok(result)) {
                cached_index_id_ = id;
                return id;
            }
        }
        
        return IndexId{0}; // Invalid ID
    }
    
    // Common index-based delete operation
    [[nodiscard]] uint32_t delete_by_index_scan(const FieldType& value, bool exact_match = true) const {
        IndexId index_id = get_index_id();
        if (index_id.inner == 0) {
            return 0; // No index available
        }
        
        SpacetimeDB::bsatn::Writer bound_writer;
        bound_writer.write_u8(0); // Bound::Included
        SpacetimeDB::bsatn::serialize(bound_writer, value);
        auto bound_buffer = bound_writer.get_buffer();
        
        uint32_t deleted_count = 0;
        Status status;
        
        if (exact_match) {
            // Exact match for primary/unique keys
            status = ::datastore_delete_by_index_scan_range_bsatn(
                index_id,
                nullptr, 0, ColId{0},
                bound_buffer.data(), bound_buffer.size(),
                bound_buffer.data(), bound_buffer.size(),
                &deleted_count
            );
        } else {
            // Prefix match for indexed fields
            status = ::datastore_delete_by_index_scan_range_bsatn(
                index_id,
                bound_buffer.data(), bound_buffer.size(), ColId{1},
                nullptr, 0,
                nullptr, 0,
                &deleted_count
            );
        }
        
        return is_ok(status) ? deleted_count : 0;
    }
    
    // Common index-based update operation
    bool update_by_index(const TableType& new_row) const {
        IndexId index_id = get_index_id();
        auto result = this->get_table().update_by_index(index_id, new_row);
        return result.has_value();
    }
    
    // Must be implemented by derived classes
    virtual IndexId get_index_id() const = 0;
    
public:
    using table_type = TableType;
    using field_type = FieldType;
    
    TypedFieldAccessor(const char* table_name, const char* field_name, FieldType TableType::*ptr) 
        : SpacetimeDB::TableAccessor<TableType>(table_name), 
          field_name_(field_name), 
          member_ptr_(ptr) {}
    
    [[nodiscard]] constexpr std::string_view field_name() const { return field_name_; }
    [[nodiscard]] constexpr auto member_pointer() const { return member_ptr_; }
    
    // Common operations available to all field types
    // std::vector<TableType> filter(const FieldType& value) const {
    //     return this->get_table().filter([this, &value](const TableType& row) {
    //         return get_field_value(row) == value;
    //     });
    // }

    uint32_t delete_by_value(const FieldType& value) const {
        return delete_by_index_scan(value, true);
    }
    
    // DatabaseResult<TableType> try_insert(const TableType& row) const {
    //     // Without exceptions, we just call insert and return success
    //     // If there's a constraint violation, insert will abort
    //     this->insert(row);
    //     return DatabaseResult<TableType>(std::in_place_index<0>, row);
    // }
};

// =============================================================================
// Specialized Field Accessors (Minimal Duplication)
// =============================================================================

template<typename TableType, typename FieldType>
class TypedPrimaryKeyAccessor : public TypedFieldAccessor<TableType, FieldType> {
private:
    [[nodiscard]] IndexId get_index_id() const override {
        auto patterns = detail::get_index_patterns(
            std::string(this->table_name_), 
            std::string(this->field_name_),
            FieldConstraint::PrimaryKey
        );
        IndexId id = this->resolve_index_with_patterns({patterns[0], patterns[1], patterns[2]});
        
        if (id.inner == 0) {
            std::abort(); // Failed to resolve index ID for primary key field
        }
        return id;
    }
    
public:
    using TypedFieldAccessor<TableType, FieldType>::TypedFieldAccessor;
    
    [[nodiscard]] std::optional<TableType> find(const FieldType& key_value) const {
        IndexId index_id = get_index_id();
        if (index_id.inner != 0) {
            // Use efficient index-based iteration
            IndexIterator<TableType> iter(index_id, key_value);
            if (iter != IndexIterator<TableType>()) {
                return *iter;
            } else {
                return std::nullopt;
            }
        }
        return std::nullopt;
        // // Fallback to iteration
        // return SpacetimeDB::TableAccessor<TableType>::find([&](const TableType& row) {
        //     return this->get_field_value(row) == key_value;
        // });
    }

    bool delete_by_key(const FieldType& key_value) const {
        uint32_t count = this->delete_by_index_scan(key_value, true);
        if (count > 0) return true;

        return false;
        // // Fallback to iteration
        // return this->delete_where_primary_key([&](const TableType& row) {
        //     return this->get_field_value(row) == key_value;
        // });
    }
    
    bool update(const TableType& new_row) const {
        if (this->update_by_index(new_row)) return true;
        
        // Fallback
        FieldType key_val = this->get_field_value(new_row);
        auto existing = find(key_val);
        if (existing) {
            this->update_by_value(*existing, new_row);
            return true;
        }
        return false;
    }
    
    TableType try_insert_or_update(const TableType& row) const {
        FieldType key_val = this->get_field_value(row);
        auto existing = find(key_val);
        if (existing) {
            auto _ = update(row);
            return row;
        } else {
            return this->insert(row);
        }
    }
};

template<typename TableType, typename FieldType>
class TypedUniqueAccessor : public TypedFieldAccessor<TableType, FieldType> {
private:
    [[nodiscard]] IndexId get_index_id() const override {
        auto patterns = detail::get_index_patterns(
            std::string(this->table_name_), 
            std::string(this->field_name_),
            FieldConstraint::Unique
        );
        IndexId id = this->resolve_index_with_patterns({patterns[0], patterns[1], patterns[2]});
        
        if (id.inner == 0) {
            std::abort(); // Failed to resolve index ID for unique field
        }
        return id;
    }
    
public:
    using TypedFieldAccessor<TableType, FieldType>::TypedFieldAccessor;
    
    std::optional<TableType> find(const FieldType& value) const {
        IndexId index_id = get_index_id();
        if (index_id.inner != 0) {
            // Use efficient index-based iteration
            IndexIterator<TableType> iter(index_id, value);
            if (iter != IndexIterator<TableType>()) {
                return *iter;
            } else {
                return std::nullopt;
            }
        }
        return std::nullopt;

        // return SpacetimeDB::TableAccessor<TableType>::find([&](const TableType& row) {
        //     return this->get_field_value(row) == value;
        // });
    }
    
    bool delete_by_value(const FieldType& value) const {
        uint32_t count = this->delete_by_index_scan(value, true);
        if (count > 0) return true;
        
        // Fallback
        auto match = find(value);
        return match ? SpacetimeDB::TableAccessor<TableType>::delete_by_value(*match) > 0 : false;
    }
    
    bool update(const TableType& new_row) const {
        if (this->update_by_index(new_row)) return true;
        
        // Fallback
        FieldType field_val = this->get_field_value(new_row);
        auto existing = find(field_val);
        if (existing) {
            this->TableAccessor<TableType>::update_by_value(*existing, new_row);
            return true;
        }
        return false;
    }
};

template<typename TableType, typename FieldType>
class TypedIndexedAccessor : public TypedFieldAccessor<TableType, FieldType> {
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
        return IndexId{0};
    }
    
public:
    using TypedFieldAccessor<TableType, FieldType>::TypedFieldAccessor;
    
    /**
     * @brief Filter rows by exact field value using index
     * 
     * Returns lazy IndexIterator - results are evaluated incrementally during iteration
     * without materializing all matches in memory.
     * 
     * @param value The field value to match exactly
     * @return IndexIterator supporting range-based for loops
     * 
     * @example Clean range-based for loop (no materialization):
     * @code
     * for (const auto& row : ctx.db[table_field].filter(value)) {
     *     // Process matching rows one at a time
     * }
     * @endcode
     * 
     * @example Materialize all results when needed:
     * @code
     * auto all_matches = ctx.db[table_field].filter(value).collect();
     * @endcode
     */
    IndexIteratorRange<TableType> filter(const FieldType& value) const {
        IndexId index_id = get_index_id();
        
        if (index_id.inner != 0) {
            // Use efficient index-based iteration
            return IndexIteratorRange<TableType>(IndexIterator<TableType>(index_id, value));
        }

        return IndexIteratorRange<TableType>(IndexIterator<TableType>());
    }
    
    /**
     * @brief Filter rows by range using index
     * 
     * Returns lazy IndexIterator - results are evaluated incrementally during iteration
     * without materializing all matches in memory.
     * 
     * @tparam RangeType Range type (range_from, range_to, range_inclusive, etc.)
     * @param range The range bounds to match
     * @return IndexIterator supporting range-based for loops
     * 
     * @example Query rows in a range:
     * @code
     * auto age_range = range_from(uint8_t(18));
     * for (const auto& row : ctx.db[person_age].filter(age_range)) {
     *     // Process persons aged 18+
     * }
     * @endcode
     */
    template<typename RangeType>
    std::enable_if_t<is_range_v<RangeType>, IndexIteratorRange<TableType>>
    filter(const RangeType& range) const {
        IndexId index_id = get_index_id();
        
        if (index_id.inner != 0) {
            // Use efficient index-based iteration with range
            return IndexIteratorRange<TableType>(IndexIterator<TableType>(index_id, range));
        }

        return IndexIteratorRange<TableType>(IndexIterator<TableType>());
    }
    
    uint32_t delete_all(const FieldType& value) const {
        IndexId index_id = get_index_id();
        if (index_id.inner == 0) {
            return 0; // No index available
        }
        
        // Serialize the value to search for
        SpacetimeDB::bsatn::Writer writer;
        SpacetimeDB::bsatn::serialize(writer, value);
        auto buffer = writer.get_buffer();
        
        uint32_t deleted_count = 0;
        Status status = ::datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buffer.data(), buffer.size(),
            &deleted_count
        );
        
        return is_ok(status) ? deleted_count : 0;
    }
};

// =============================================================================
// Multi-Column Index Accessor
// =============================================================================

template<typename TableType>
class TypedMultiColumnIndexAccessor : public TableAccessor<TableType> {
private:
    std::string table_name_;
    std::string index_name_;  // This is the accessor name like "by_player_and_level"
    std::string column_list_;  // This is the column list like "player_id_level"
    mutable std::optional<IndexId> cached_index_id_;
    
    IndexId resolve_index_id() const {
        if (cached_index_id_) {
            return *cached_index_id_;
        }
        
        // Try to resolve index with multiple name patterns
        auto try_pattern = [this](const std::string& pattern) -> IndexId {
            IndexId id;
            if (is_ok(::index_id_from_name(
                reinterpret_cast<const uint8_t*>(pattern.data()),
                pattern.length(),
                &id))) {
                return id;
            }
            return IndexId{0};
        };
        
        // Try patterns in order of likelihood
        IndexId id = try_pattern(index_name_);  // User accessor name
        if (id.inner == 0) {
            id = try_pattern(table_name_ + "_" + column_list_ + "_idx_btree");  // Database-generated
        }
        if (id.inner == 0) {
            id = try_pattern(table_name_ + "_" + index_name_ + "_idx_btree");  // Accessor-based
        }
        
        cached_index_id_ = id;
        return id;
    }
    
public:
    TypedMultiColumnIndexAccessor(const char* table_name, const char* index_name, const char* column_list)
        : TableAccessor<TableType>(table_name), 
          table_name_(table_name),
          index_name_(index_name),
          column_list_(column_list) {}
    
    // Exact match on all columns (template method - types deduced from call)
    template<typename... FieldTypes>
    IndexIteratorRange<TableType> filter(const std::tuple<FieldTypes...>& values) const 
        requires (sizeof...(FieldTypes) > 0 && sizeof...(FieldTypes) <= 6)
    {
        IndexId id = resolve_index_id();
        
        if (id.inner == 0) {
            return IndexIteratorRange<TableType>(IndexIterator<TableType>());
        }
        
        return IndexIteratorRange<TableType>(IndexIterator<TableType>(id, values));
    }
    
    // Prefix-only match: find all rows where first N-1 columns match
    template<typename FirstColType>
    IndexIteratorRange<TableType> filter(const FirstColType& prefix_value) const 
        requires (!is_tuple_v<FirstColType>)
    {
        IndexId id = resolve_index_id();
        
        if (id.inner == 0) {
            return IndexIteratorRange<TableType>(IndexIterator<TableType>());
        }
        
        // Use prefix_match_tag to disambiguate constructor
        return IndexIteratorRange<TableType>(IndexIterator<TableType>(prefix_match_tag{}, id, prefix_value));
    }
    
    // Prefix + range match: find rows where first N-1 columns match and last is in range
    template<typename FirstColType, typename RangeType>
    IndexIteratorRange<TableType> filter(const std::tuple<FirstColType, RangeType>& values) const 
        requires (is_range_v<RangeType>)
    {
        IndexId id = resolve_index_id();
        
        if (id.inner == 0) {
            return IndexIteratorRange<TableType>(IndexIterator<TableType>());
        }
        
        return IndexIteratorRange<TableType>(IndexIterator<TableType>(id, values));
    }
};

// =============================================================================
// Auto-Increment Integration Helper
// =============================================================================

// Helper macro to register auto-increment integration function
// Creates a unique function and registers it for the struct type
#define SPACETIMEDB_AUTOINC_INTEGRATION_IMPL(StructType, field_name) \
    namespace SpacetimeDB { namespace detail { \
        static void SPACETIMEDB_PASTE(autoinc_integrate_, __LINE__)(StructType& row, SpacetimeDB::bsatn::Reader& reader) { \
            using FieldType = decltype(std::declval<StructType>().field_name); \
            FieldType generated_value = SpacetimeDB::bsatn::deserialize<FieldType>(reader); \
            row.field_name = generated_value; \
        } \
    }} \
    extern "C" __attribute__((export_name("__preinit__19_autoinc_register_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__19_autoinc_register_, __LINE__)() { \
        SpacetimeDB::detail::get_autoinc_integrator<StructType>() = \
            &SpacetimeDB::detail::SPACETIMEDB_PASTE(autoinc_integrate_, __LINE__); \
    }

// =============================================================================
// Field Constraint Registration Macros
// =============================================================================

#define FIELD_PrimaryKey(table_name, field_name) \
    static constexpr SpacetimeDB::PrimaryKeyFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                      decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDB::FieldConstraint::PrimaryKey); \
    }

#define FIELD_Unique(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(FilterableValue<FieldType>, \
            "Field '" #field_name "' cannot have Unique constraint - type is not filterable."); \
        return true; \
    }(), "Constraint validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDB::UniqueFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                  decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDB::FieldConstraint::Unique); \
    }

#define FIELD_Index(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(FilterableValue<FieldType>, \
            "Field '" #field_name "' cannot have Index constraint - type is not filterable."); \
        return true; \
    }(), "Constraint validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDB::IndexedFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                   decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDB::FieldConstraint::Indexed); \
    }

#define FIELD_PrimaryKeyAutoInc(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(AutoIncrementable<FieldType>, \
            "Field '" #field_name "' cannot have AutoIncrement constraint - type is not auto-incrementable."); \
        return true; \
    }(), "AutoIncrement validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDB::PrimaryKeyFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                      decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, static_cast<::SpacetimeDB::FieldConstraint>( \
                static_cast<int>(::SpacetimeDB::FieldConstraint::PrimaryKey) | static_cast<int>(::SpacetimeDB::FieldConstraint::AutoInc))); \
    } \
    SPACETIMEDB_AUTOINC_INTEGRATION_IMPL(typename std::remove_cv_t<decltype(table_name)>::type, field_name)

#define FIELD_UniqueAutoInc(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(FilterableValue<FieldType>, \
            "Field '" #field_name "' cannot have Unique constraint - type is not filterable."); \
        static_assert(AutoIncrementable<FieldType>, \
            "Field '" #field_name "' cannot have AutoIncrement constraint - type is not auto-incrementable."); \
        return true; \
    }(), "Constraint validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDB::UniqueFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                  decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, static_cast<::SpacetimeDB::FieldConstraint>( \
                static_cast<int>(::SpacetimeDB::FieldConstraint::Unique) | static_cast<int>(::SpacetimeDB::FieldConstraint::AutoInc))); \
    } \
    SPACETIMEDB_AUTOINC_INTEGRATION_IMPL(typename std::remove_cv_t<decltype(table_name)>::type, field_name)

#define FIELD_IndexAutoInc(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(FilterableValue<FieldType>, \
            "Field '" #field_name "' cannot have Index constraint - type is not filterable."); \
        static_assert(AutoIncrementable<FieldType>, \
            "Field '" #field_name "' cannot have AutoIncrement constraint - type is not auto-incrementable."); \
        return true; \
    }(), "Constraint validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDB::IndexedFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                   decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, static_cast<::SpacetimeDB::FieldConstraint>( \
                static_cast<int>(::SpacetimeDB::FieldConstraint::Indexed) | static_cast<int>(::SpacetimeDB::FieldConstraint::AutoInc))); \
    }

#define FIELD_AutoInc(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(AutoIncrementable<FieldType>, \
            "Field '" #field_name "' cannot have AutoIncrement constraint - type is not auto-incrementable."); \
        return true; \
    }(), "AutoIncrement validation for " #table_name "." #field_name); \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDB::FieldConstraint::AutoInc); \
    } \
    SPACETIMEDB_AUTOINC_INTEGRATION_IMPL(typename std::remove_cv_t<decltype(table_name)>::type, field_name)

// Helper to join field names with underscores at compile time
#define SPACETIMEDB_JOIN_FIELDS(...) SPACETIMEDB_JOIN_FIELDS_IMPL(__VA_ARGS__)
#define SPACETIMEDB_JOIN_FIELDS_IMPL(...) SPACETIMEDB_GET_JOIN_MACRO(__VA_ARGS__, \
    SPACETIMEDB_JOIN_6, SPACETIMEDB_JOIN_5, SPACETIMEDB_JOIN_4, \
    SPACETIMEDB_JOIN_3, SPACETIMEDB_JOIN_2, SPACETIMEDB_JOIN_1)(__VA_ARGS__)
#define SPACETIMEDB_GET_JOIN_MACRO(_1,_2,_3,_4,_5,_6,NAME,...) NAME
#define SPACETIMEDB_JOIN_1(a) #a
#define SPACETIMEDB_JOIN_2(a,b) #a "_" #b
#define SPACETIMEDB_JOIN_3(a,b,c) #a "_" #b "_" #c
#define SPACETIMEDB_JOIN_4(a,b,c,d) #a "_" #b "_" #c "_" #d
#define SPACETIMEDB_JOIN_5(a,b,c,d,e) #a "_" #b "_" #c "_" #d "_" #e
#define SPACETIMEDB_JOIN_6(a,b,c,d,e,f) #a "_" #b "_" #c "_" #d "_" #e "_" #f

// Multi-column index registration macro
#define FIELD_NamedMultiColumnIndex(table_name, index_name, ...) \
    static constexpr auto table_name##_##index_name = SpacetimeDB::MultiColumnIndexTag< \
        typename std::remove_cv_t<decltype(table_name)>::type \
    >{#table_name, #index_name, SPACETIMEDB_JOIN_FIELDS(__VA_ARGS__)}; \
    extern "C" __attribute__((export_name("__preinit__21_field_multi_index_" #table_name "_" #index_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_multi_index_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(index_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDB::Internal::getV9Builder().AddMultiColumnIndex<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #index_name, {SPACETIMEDB_STRINGIFY_EACH(__VA_ARGS__)}); \
    }

#define FIELD_Default(table_name, field_name, default_value) \
    extern "C" __attribute__((export_name("__preinit__21_field_default_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_default_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        \
        /* Serialize the default value to BSATN bytes */ \
        auto serialized = SpacetimeDB::bsatn::to_bytes(default_value); \
        \
        SpacetimeDB::Internal::getV9Builder().AddColumnDefault<TableType>( \
            #table_name, \
            #field_name, \
            serialized \
        ); \
    }
} // namespace SpacetimeDB
