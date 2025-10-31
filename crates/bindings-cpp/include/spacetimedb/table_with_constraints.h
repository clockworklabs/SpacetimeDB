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

namespace SpacetimeDb {

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
    SpacetimeDb::Internal::Module::RegisterTableInternalImpl<T>(name, is_public, constraints);
}

// template<typename T>
// void register_scheduled_table_type_with_constraints(const char* name, const char* reducer_name, bool is_public,
//                                                    const std::vector<FieldConstraintInfo>& constraints) {
//     register_table_type_with_constraints<T>(name, is_public, constraints);
    
//     auto& module_def = SpacetimeDb::Internal::Module::GetModuleDef();
//     auto it = module_def.table_indices.find(&typeid(T));
//     if (it != module_def.table_indices.end()) {
//         auto& table = module_def.tables[it->second];
        
//         uint16_t scheduled_at_column = UINT16_MAX;
//         for (size_t i = 0; i < table.fields.size(); ++i) {
//             if (table.fields[i].name == std::string("scheduled_at")) {
//                 scheduled_at_column = static_cast<uint16_t>(i);
//                 break;
//             }
//         }
        
//         if (scheduled_at_column == UINT16_MAX) {
//             std::abort(); // Scheduled table must have a 'scheduled_at' field of type ScheduleAt
//         }
        
//         SpacetimeDb::Internal::RawScheduleDefV9 schedule_def;
//         schedule_def.name = std::nullopt;
//         schedule_def.reducer_name = reducer_name;
//         schedule_def.scheduled_at_column = scheduled_at_column;
        
//         table.schedule = new SpacetimeDb::Internal::RawScheduleDefV9(std::move(schedule_def));
//     }
// }

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
        bool is_public = (access_enum == SpacetimeDb::Internal::TableAccess::Public); \
        SpacetimeDb::Module::RegisterTable<type>(#table_name, is_public); \
    } \
    struct SPACETIMEDB_PASTE(table_name, _tag_type) : SpacetimeDb::TableTag<type> { \
        static constexpr const char* __table_name_internal = #table_name; \
    }; \
    constexpr SPACETIMEDB_PASTE(table_name, _tag_type) table_name{};

/**
 * @brief Schedule a table for automatic reducer execution
 */
#define SPACETIMEDB_SCHEDULE(table_name, scheduled_at_column_index, reducer_name) \
    extern "C" __attribute__((export_name("__preinit__19_schedule_" #table_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__19_schedule_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_line_, __LINE__)))() { \
        SpacetimeDb::Internal::getV9Builder().RegisterSchedule(#table_name, scheduled_at_column_index, #reducer_name); \
    }

// =============================================================================
// Field Tag System
// =============================================================================

template<typename TableType, typename FieldType, SpacetimeDb::FieldConstraint Constraint>
struct FieldTag {
    const char* field_name;
    const char* table_name;
    FieldType TableType::*member_ptr;
    
    static constexpr SpacetimeDb::FieldConstraint constraint = Constraint;
    using table_type = TableType;
    using field_type = FieldType;
    
    constexpr FieldTag(const char* table, const char* field, FieldType TableType::*ptr) 
        : field_name(field), table_name(table), member_ptr(ptr) {}
};

template<typename TableType, typename FieldType>
using PrimaryKeyFieldTag = FieldTag<TableType, FieldType, SpacetimeDb::FieldConstraint::PrimaryKey>;

template<typename TableType, typename FieldType>  
using UniqueFieldTag = FieldTag<TableType, FieldType, SpacetimeDb::FieldConstraint::Unique>;

template<typename TableType, typename FieldType>
using IndexedFieldTag = FieldTag<TableType, FieldType, SpacetimeDb::FieldConstraint::Indexed>;

// =============================================================================
// Constraint Concepts
// =============================================================================

template<typename T>
concept FilterableValue = 
    std::integral<T> ||
    std::same_as<T, std::string> ||
    std::same_as<T, SpacetimeDb::Identity> ||
    std::same_as<T, SpacetimeDb::ConnectionId> ||
    std::same_as<T, SpacetimeDb::Timestamp> ||
    std::same_as<T, SpacetimeDb::I128> ||
    std::same_as<T, SpacetimeDb::U128> ||
    std::same_as<T, SpacetimeDb::I256> ||
    std::same_as<T, SpacetimeDb::U256> ||
    std::same_as<T, SpacetimeDb::i256> ||
    std::same_as<T, SpacetimeDb::u256> ||
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
    std::same_as<T, SpacetimeDb::I128> ||
    std::same_as<T, SpacetimeDb::U128> ||
    std::same_as<T, SpacetimeDb::i256> ||
    std::same_as<T, SpacetimeDb::u256>;

// =============================================================================
// Unified Field Accessor Base Class
// =============================================================================

template<typename TableType, typename FieldType>
class TypedFieldAccessor : public SpacetimeDb::TableAccessor<TableType> {
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
        
        SpacetimeDb::bsatn::Writer bound_writer;
        bound_writer.write_u8(0); // Bound::Included
        SpacetimeDb::bsatn::serialize(bound_writer, value);
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
    [[nodiscard]] bool update_by_index(const TableType& new_row) const {
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
        : SpacetimeDb::TableAccessor<TableType>(table_name), 
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
        // return SpacetimeDb::TableAccessor<TableType>::find([&](const TableType& row) {
        //     return this->get_field_value(row) == key_value;
        // });
    }

    [[nodiscard]] bool delete_by_key(const FieldType& key_value) const {
        uint32_t count = this->delete_by_index_scan(key_value, true);
        if (count > 0) return true;

        return false;
        // // Fallback to iteration
        // return this->delete_where_primary_key([&](const TableType& row) {
        //     return this->get_field_value(row) == key_value;
        // });
    }
    
    [[nodiscard]] bool update(const TableType& new_row) const {
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

        // return SpacetimeDb::TableAccessor<TableType>::find([&](const TableType& row) {
        //     return this->get_field_value(row) == value;
        // });
    }
    
    bool delete_by_value(const FieldType& value) const {
        uint32_t count = this->delete_by_index_scan(value, true);
        if (count > 0) return true;
        
        // Fallback
        auto match = find(value);
        return match ? SpacetimeDb::TableAccessor<TableType>::delete_by_value(*match) > 0 : false;
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
    
    // Override filter to use index-based iteration for efficiency
    std::vector<TableType> filter(const FieldType& value) const {
        IndexId index_id = get_index_id();
        
        if (index_id.inner != 0) {
            // Use efficient index-based iteration
            std::vector<TableType> results;
            for (IndexIterator<TableType> iter(index_id, value); iter != IndexIterator<TableType>(); ++iter) {
                results.push_back(*iter);
            }
            return results;
        }

        return std::vector<TableType>{};
        // // Fallback to base implementation if index not found
        // return TypedFieldAccessor<TableType, FieldType>::filter(value);
    }
    
    // Filter by range using index (new functionality!)
    template<typename RangeType>
    std::enable_if_t<is_range_v<RangeType>, std::vector<TableType>>
    filter(const RangeType& range) const {
        IndexId index_id = get_index_id();
        
        if (index_id.inner != 0) {
            // Use efficient index-based iteration with range
            std::vector<TableType> results;
            for (IndexIterator<TableType> iter(index_id, range); iter != IndexIterator<TableType>(); ++iter) {
                results.push_back(*iter);
            }
            return results;
        }

        return std::vector<TableType>{};
        // // Fallback to manual filtering if index not found
        // return this->get_table().filter([this, &range](const TableType& row) {
        //     return range.contains(this->get_field_value(row));
        // });
    }
    
    uint32_t delete_all(const FieldType& value) const {
        uint32_t count = this->delete_by_index_scan(value, false);
        if (count > 0) return count;
        
        // Fallback
        auto matches = this->filter(value);
        uint32_t deleted_count = 0;
        for (const auto& row : matches) {
            if (this->delete_by_value(row) > 0) {
                deleted_count++;
            }
        }
        return deleted_count;
    }
};

// =============================================================================
// Auto-Increment Integration Helper
// =============================================================================

// Helper macro to register auto-increment integration function
// Creates a unique function and registers it for the struct type
#define SPACETIMEDB_AUTOINC_INTEGRATION_IMPL(StructType, field_name) \
    namespace SpacetimeDb { namespace detail { \
        static void SPACETIMEDB_PASTE(autoinc_integrate_, __LINE__)(StructType& row, SpacetimeDb::bsatn::Reader& reader) { \
            using FieldType = decltype(std::declval<StructType>().field_name); \
            FieldType generated_value = SpacetimeDb::bsatn::deserialize<FieldType>(reader); \
            row.field_name = generated_value; \
        } \
    }} \
    extern "C" __attribute__((export_name("__preinit__19_autoinc_register_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__19_autoinc_register_, __LINE__)() { \
        SpacetimeDb::detail::get_autoinc_integrator<StructType>() = \
            &SpacetimeDb::detail::SPACETIMEDB_PASTE(autoinc_integrate_, __LINE__); \
    }

// =============================================================================
// Field Constraint Registration Macros
// =============================================================================

#define FIELD_PrimaryKey(table_name, field_name) \
    static constexpr SpacetimeDb::PrimaryKeyFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                      decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDb::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDb::FieldConstraint::PrimaryKey); \
    }

#define FIELD_Unique(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(FilterableValue<FieldType>, \
            "Field '" #field_name "' cannot have Unique constraint - type is not filterable."); \
        return true; \
    }(), "Constraint validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDb::UniqueFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                  decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDb::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDb::FieldConstraint::Unique); \
    }

#define FIELD_Index(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(FilterableValue<FieldType>, \
            "Field '" #field_name "' cannot have Index constraint - type is not filterable."); \
        return true; \
    }(), "Constraint validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDb::IndexedFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                   decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDb::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDb::FieldConstraint::Indexed); \
    }

#define FIELD_PrimaryKeyAutoInc(table_name, field_name) \
    static_assert([]() constexpr { \
        using TableType = typename std::remove_cv_t<decltype(table_name)>::type; \
        using FieldType = decltype(std::declval<TableType>().field_name); \
        static_assert(AutoIncrementable<FieldType>, \
            "Field '" #field_name "' cannot have AutoIncrement constraint - type is not auto-incrementable."); \
        return true; \
    }(), "AutoIncrement validation for " #table_name "." #field_name); \
    static constexpr SpacetimeDb::PrimaryKeyFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                      decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDb::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, static_cast<::SpacetimeDb::FieldConstraint>( \
                static_cast<int>(::SpacetimeDb::FieldConstraint::PrimaryKey) | static_cast<int>(::SpacetimeDb::FieldConstraint::AutoInc))); \
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
    static constexpr SpacetimeDb::UniqueFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                  decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDb::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, static_cast<::SpacetimeDb::FieldConstraint>( \
                static_cast<int>(::SpacetimeDb::FieldConstraint::Unique) | static_cast<int>(::SpacetimeDb::FieldConstraint::AutoInc))); \
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
    static constexpr SpacetimeDb::IndexedFieldTag<typename std::remove_cv_t<decltype(table_name)>::type, \
                                                   decltype(std::declval<typename std::remove_cv_t<decltype(table_name)>::type>().field_name)> \
    table_name##_##field_name { #table_name, #field_name, &std::remove_cv_t<decltype(table_name)>::type::field_name }; \
    extern "C" __attribute__((export_name("__preinit__21_field_constraint_" #table_name "_" #field_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_constraint_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(field_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDb::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, static_cast<::SpacetimeDb::FieldConstraint>( \
                static_cast<int>(::SpacetimeDb::FieldConstraint::Indexed) | static_cast<int>(::SpacetimeDb::FieldConstraint::AutoInc))); \
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
        SpacetimeDb::Internal::getV9Builder().AddFieldConstraint<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #field_name, ::SpacetimeDb::FieldConstraint::AutoInc); \
    } \
    SPACETIMEDB_AUTOINC_INTEGRATION_IMPL(typename std::remove_cv_t<decltype(table_name)>::type, field_name)

#define FIELD_NamedMultiColumnIndex(table_name, index_name, field1, field2) \
    extern "C" __attribute__((export_name("__preinit__21_field_multi_index_" #table_name "_" #index_name "_line_" SPACETIMEDB_STRINGIFY(__LINE__)))) \
    void SPACETIMEDB_PASTE(__preinit__21_field_multi_index_, SPACETIMEDB_PASTE(table_name, SPACETIMEDB_PASTE(_, SPACETIMEDB_PASTE(index_name, SPACETIMEDB_PASTE(_line_, __LINE__)))))() { \
        SpacetimeDb::Internal::getV9Builder().AddMultiColumnIndex<typename std::remove_cv_t<decltype(table_name)>::type>( \
            #table_name, #index_name, {#field1, #field2}); \
    }

} // namespace SpacetimeDb
