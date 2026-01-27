// Include autogen types first to ensure complete type definitions
#include "spacetimedb/internal/autogen/SumType.g.h"
#include "spacetimedb/internal/autogen/SumTypeVariant.g.h"
#include "spacetimedb/internal/autogen/ProductType.g.h"
#include "spacetimedb/internal/autogen/ProductTypeElement.g.h"
#include "spacetimedb/internal/autogen/AlgebraicType.g.h"
#include "spacetimedb/internal/autogen/TableType.g.h"
#include "spacetimedb/internal/autogen/TableAccess.g.h"
#include "spacetimedb/internal/autogen/RawIndexDefV9.g.h"
#include "spacetimedb/internal/autogen/RawTypeDefV9.g.h"
#include "spacetimedb/internal/autogen/RawConstraintDefV9.g.h"
#include "spacetimedb/internal/autogen/RawConstraintDataV9.g.h"
#include "spacetimedb/internal/autogen/RawUniqueConstraintDataV9.g.h"
#include "spacetimedb/internal/autogen/RawIndexAlgorithm.g.h"
#include "spacetimedb/internal/autogen/RawTableDefV9.g.h"
#include "spacetimedb/internal/autogen/RawReducerDefV9.g.h"
#include "spacetimedb/internal/autogen/RawSequenceDefV9.g.h"
#include "spacetimedb/internal/autogen/RawScheduleDefV9.g.h"
#include "spacetimedb/internal/autogen/RawRowLevelSecurityDefV9.g.h"
#include "spacetimedb/internal/autogen/Lifecycle.g.h"

// Then include the main headers
#include "spacetimedb/internal/v9_builder.h"
#include "spacetimedb/internal/v9_type_registration.h"
#include "spacetimedb/internal/debug.h"
#include "spacetimedb/bsatn/bsatn.h"
#include "spacetimedb/internal/Module_impl.h"
#include <functional>
#include <cstdio>
#include <cxxabi.h>
#include <memory>
#include <algorithm>
#include <set>

namespace SpacetimeDB {
namespace Internal {

// Forward declaration of the global V9 module accessor
RawModuleDefV9& GetV9Module();

// demangle_cpp_type_name function removed - using global version from v9_type_registration.cpp

// Global V9Builder instance
std::unique_ptr<V9Builder> g_v9_builder;

void initializeV9Builder() {
    g_v9_builder = std::make_unique<V9Builder>();
    // Also initialize the type registration system
    initializeV9TypeRegistration();
}

V9Builder& getV9Builder() {
    if (!g_v9_builder) {
        STDB_DEBUG("Initializing V9Builder");
        initializeV9Builder();
    }
    return *g_v9_builder;
}

// Constructor
V9Builder::V9Builder() {
    // No TypeRegistry anymore - everything goes through V9TypeRegistration
}

/**
 * Register a type using the unified type registration system
 * This is now just a thin wrapper around the V9TypeRegistration system
 */
AlgebraicType V9Builder::registerType(const bsatn::AlgebraicType& bsatn_type,
                                      const std::string& explicit_name,
                                      const std::type_info* cpp_type) {
    return getV9TypeRegistration().registerType(bsatn_type, explicit_name, cpp_type);
}

void V9Builder::AddV9Table(const std::string& table_name,
                                  const bsatn::AlgebraicType& table_type,
                                  const std::type_info* cpp_type,
                                  bool is_public,
                                  const std::vector<uint16_t>& primary_key,
                                  const std::vector<RawIndexDefV9>& indexes,
                                  const std::vector<RawConstraintDefV9>& constraints,
                                  const std::vector<RawSequenceDefV9>& sequences,
                                  const std::optional<RawScheduleDefV9>& schedule) {
    
    // Register the table type using the unified system
    // Use empty string to let the system extract the struct name from cpp_type
    AlgebraicType registered_type = registerType(table_type, "", cpp_type);
    
    // Extract the typespace index from the registered type
    uint32_t type_ref;
    if (registered_type.get_tag() == AlgebraicType::Tag::Ref) {
        type_ref = registered_type.get<0>();
    } else {
        // This shouldn't happen for a table type - tables should always be complex types
        fprintf(stderr, "ERROR: Table '%s' did not register as a complex type\n", table_name.c_str());
        type_ref = 0;
    }
    
    // RegisterTable now handles all constraint and index generation,
    // so we just use what was passed in directly
    
    // Create the table definition
    RawTableDefV9 table_def;
    table_def.name = table_name;
    table_def.product_type_ref = type_ref;
    table_def.primary_key = primary_key;
    table_def.indexes = indexes;  // Use indexes passed from RegisterTable
    table_def.constraints = constraints;  // Use constraints passed from RegisterTable
    table_def.sequences = sequences;
    table_def.schedule = schedule;
    table_def.table_type = TableType::User;  // User-defined table
    table_def.table_access = is_public ? TableAccess::Public : TableAccess::Private;
    
    // Check if table already exists to prevent duplicates
    auto& module = GetV9Module();
    for (const auto& existing_table : module.tables) {
        if (existing_table.name == table_name) {
            fprintf(stdout, "DEBUG: Table '%s' already registered, skipping duplicate registration\n", 
                       table_name.c_str());
            return;
        }
    }
    
    // Debug: Show all existing table names before adding new one
    //fprintf(stdout, "DEBUG: Adding table '%s'. Existing tables: [", table_name.c_str());
    // for (size_t i = 0; i < module.tables.size(); ++i) {
    //     if (i > 0) fprintf(stdout, ", ");
    //     fprintf(stdout, "'%s'", module.tables[i].name.c_str());
    // }
    // fprintf(stdout, "]\n");

    // Add table to the global V9 module
    module.tables.push_back(table_def);
    //fprintf(stdout, "DEBUG: Added table '%s' to V9 module (type_ref=%u) with %zu indexes, %zu constraints\n", 
    //           table_name.c_str(), type_ref, indexes.size(), constraints.size());
    
    // // Debug: Print index names
    // for (const auto& idx : indexes) {
    //     if (idx.name.has_value()) {
    //         fprintf(stdout, "  Index: %s\n", idx.name.value().c_str());
    //     }
    // }
    
    // // Debug: Print constraint names  
    // for (const auto& constraint : constraints) {
    //     if (constraint.name.has_value()) {
    //         fprintf(stdout, "  Constraint: %s\n", constraint.name.value().c_str());
    //     }
    // }
}

void V9Builder::AddV9Reducer(const std::string& reducer_name,
                                   const std::vector<bsatn::AlgebraicType>& param_types,
                                   const std::vector<std::string>& param_names,
                                   const std::vector<const std::type_info*>& param_cpp_types,
                                   const std::vector<std::string>& param_type_names,
                                   std::optional<Lifecycle> lifecycle) {
    // Create the reducer definition
    RawReducerDefV9 reducer_def;
    reducer_def.name = reducer_name;
    
    // Build the params ProductType
    ProductType params;
    for (size_t i = 0; i < param_types.size(); ++i) {
        ProductTypeElement elem;
        elem.name = i < param_names.size() ? std::make_optional(param_names[i]) : std::nullopt;
        
        // Register the parameter type with proper C++ type info and name
        const std::type_info* cpp_type = i < param_cpp_types.size() ? param_cpp_types[i] : nullptr;
        const std::string& type_name = i < param_type_names.size() ? param_type_names[i] : "";
        
        // Register the parameter type using the unified system
        elem.algebraic_type = registerType(param_types[i], type_name, cpp_type);
        
        params.elements.push_back(elem);
    }
    
    reducer_def.params = params;
    reducer_def.lifecycle = lifecycle;
    
    // Add directly to the global V9 module
    GetV9Module().reducers.push_back(reducer_def);
    //fprintf(stdout, "DEBUG: Added reducer '%s' to V9 module\n", reducer_name.c_str());
}

// Helper function to get field name from Product type structure
std::string V9Builder::getFieldName(const bsatn::AlgebraicType& table_type, uint16_t column_index) const {
    if (table_type.tag() != bsatn::AlgebraicTypeTag::Product) {
        return "column" + std::to_string(column_index);
    }
    
    const auto& product = table_type.as_product();
    if (column_index >= product.elements.size()) {
        return "column" + std::to_string(column_index);
    }
    
    const auto& element = product.elements[column_index];
    if (element.name.has_value()) {
        return element.name.value();
    }
    
    return "column" + std::to_string(column_index);
}

// Helper function to generate constraints for primary key
std::vector<RawConstraintDefV9> V9Builder::generateConstraintsForPrimaryKey(
    const std::string& table_name,
    const bsatn::AlgebraicType& table_type,
    const std::vector<uint16_t>& primary_key) const {
    
    std::vector<RawConstraintDefV9> constraints;
    
    if (primary_key.empty()) {
        return constraints;  // No primary key, no constraints
    }
    
    // Create separate unique constraint for each primary key column
    for (uint16_t col_idx : primary_key) {
        // Generate field name for this primary key column
        std::string field_name = getFieldName(table_type, col_idx);
        std::string constraint_name = table_name + "_" + field_name + "_key";
        
        // Create unique constraint data for this single column
        RawUniqueConstraintDataV9 unique_data;
        unique_data.columns = {col_idx};  // Single column constraint
        
        // Create constraint data (tagged enum) and set the variant
        RawConstraintDataV9 constraint_data;
        constraint_data.set<0>(unique_data);
        
        // Create constraint definition
        RawConstraintDefV9 constraint_def;
        constraint_def.name = constraint_name;
        constraint_def.data = constraint_data;
        
        constraints.push_back(constraint_def);
    }
    
    return constraints;
}

// Helper function to generate indexes for primary key
std::vector<RawIndexDefV9> V9Builder::generateIndexesForPrimaryKey(
    const std::string& table_name,
    const bsatn::AlgebraicType& table_type,
    const std::vector<uint16_t>& primary_key) const {
    
    std::vector<RawIndexDefV9> indexes;
    
    if (primary_key.empty()) {
        return indexes;  // No primary key, no indexes
    }
    
    // Generate field name for the primary key column
    std::string field_name = getFieldName(table_type, primary_key[0]);
    std::string index_name = table_name + "_" + field_name + "_idx_btree";
    
    // Create BTree algorithm data
    RawIndexAlgorithmBTreeData btree_data;
    btree_data.columns = primary_key;
    
    // Create index algorithm (tagged enum) and set the BTree variant (index 0)
    RawIndexAlgorithm algorithm;
    algorithm.set<0>(btree_data);
    
    // Create index definition
    RawIndexDefV9 index_def;
    index_def.name = index_name;
    index_def.accessor_name = field_name;
    index_def.algorithm = algorithm;
    
    indexes.push_back(index_def);
    
    return indexes;
}

void V9Builder::RegisterSchedule(const std::string& table_name,
                                 uint16_t scheduled_at_column,
                                 const std::string& reducer_name) {
    // Store the schedule to be applied when the table is registered
    PendingSchedule schedule;
    schedule.table_name = table_name;
    schedule.scheduled_at_column = scheduled_at_column;
    schedule.reducer_name = reducer_name;
    
    pending_schedules_[table_name] = schedule;
    
    // fprintf(stdout, "DEBUG: Registered schedule for table '%s', column %u, reducer '%s'\n",
    //         table_name.c_str(), scheduled_at_column, reducer_name.c_str());
}

void V9Builder::RegisterRowLevelSecurity(const std::string& sql_query) {
    // Create an RLS definition and add it to the global module
    RawRowLevelSecurityDefV9 rls_def;
    rls_def.sql = sql_query;
    
    // Add directly to the global V9 module
    GetV9Module().row_level_security.push_back(rls_def);
    
    // fprintf(stdout, "DEBUG: Registered RLS policy: %s\n", sql_query.c_str());
}

// =============================================================================
// End of V9Builder Implementation
// =============================================================================


// Helper to find existing table by name in the module
RawTableDefV9* V9Builder::findTableByName(const std::string& table_name) {
    auto& module = GetV9Module();
    for (auto& table : module.tables) {
        if (table.name == table_name) {
            return &table;
        }
    }
    return nullptr;
}

// Helper to create a BTree index for a field
RawIndexDefV9 V9Builder::createBTreeIndex(const std::string& table_name,
                                          const std::string& field_name,
                                          uint16_t field_idx) const {
    RawIndexAlgorithmBTreeData btree_data;
    btree_data.columns = {field_idx};
    
    RawIndexAlgorithm algorithm;
    algorithm.set<0>(btree_data);  // Set BTree variant
    
    RawIndexDefV9 index_def;
    index_def.name = table_name + "_" + field_name + "_idx_btree";
    index_def.accessor_name = field_name;
    index_def.algorithm = algorithm;
    
    return index_def;
}

// Helper to create a unique constraint for a field
RawConstraintDefV9 V9Builder::createUniqueConstraint(const std::string& table_name,
                                                     const std::string& field_name,
                                                     uint16_t field_idx) const {
    RawUniqueConstraintDataV9 unique_data;
    unique_data.columns = {field_idx};
    
    RawConstraintDataV9 constraint_data;
    constraint_data.set<0>(unique_data);  // Set the Unique variant
    
    RawConstraintDefV9 constraint_def;
    constraint_def.name = table_name + "_" + field_name + "_key";
    constraint_def.data = std::move(constraint_data);
    
    return constraint_def;
}


} // namespace Internal
} // namespace SpacetimeDB