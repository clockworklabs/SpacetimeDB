#ifndef SPACETIMEDB_V9_BUILDER_H
#define SPACETIMEDB_V9_BUILDER_H

#include <memory>
#include <vector>
#include <optional>
#include <typeinfo>
#include <cstring>
#include <tuple>
#include "autogen/RawModuleDefV9.g.h"
#include "autogen/AlgebraicType.g.h"
#include "autogen/RawTableDefV9.g.h"
#include "autogen/RawReducerDefV9.g.h"
#include "autogen/RawConstraintDefV9.g.h"
#include "autogen/RawSequenceDefV9.g.h"
#include "autogen/RawScheduleDefV9.g.h"
#include "autogen/RawTypeDefV9.g.h"
#include "autogen/RawIndexDefV9.g.h"
#include "autogen/RawConstraintDataV9.g.h"
#include "autogen/RawIndexAlgorithm.g.h"  // Contains RawIndexAlgorithmBTreeData
#include "autogen/RawUniqueConstraintDataV9.g.h"
#include "autogen/ProductType.g.h"
#include "autogen/Lifecycle.g.h"
#include "autogen/RawColumnDefaultValueV9.g.h"
#include "autogen/RawMiscModuleExportV9.g.h"
#include "autogen/RawViewDefV9.g.h"
#include "../bsatn/bsatn.h"
#include "../database.h"  // For FieldConstraintInfo
#include "field_registration.h"  // For get_table_descriptors
#include "v9_type_registration.h"  // For getV9TypeRegistration
#include "../reducer_error.h"  // For Outcome
#include "buffer_pool.h"  // For IterBuf

namespace SpacetimeDB {

// Forward declarations for view context types (defined in view_context.h)
struct ViewContext;
struct AnonymousViewContext;

// Forward declaration for procedure context type (defined in procedure_context.h)
struct ProcedureContext;

// Forward declare fail_reducer from reducer_error.h for use in templates
void fail_reducer(std::string message);

namespace Internal {

// Forward declare the handler registration function from Module.cpp
void RegisterReducerHandler(const std::string& name, 
                           std::function<void(ReducerContext&, BytesSource)> handler,
                           std::optional<Lifecycle> lifecycle = std::nullopt);

// Forward declare view handler registration functions from Module.cpp
void RegisterViewHandler(const std::string& name,
                        std::function<std::vector<uint8_t>(ViewContext&, BytesSource)> handler);
void RegisterAnonymousViewHandler(const std::string& name,
                                 std::function<std::vector<uint8_t>(AnonymousViewContext&, BytesSource)> handler);

// Get the number of registered view handlers
size_t GetViewHandlerCount();
size_t GetAnonymousViewHandlerCount();

// Forward declare procedure handler registration function from Module.cpp
void RegisterProcedureHandler(const std::string& name,
                             std::function<std::vector<uint8_t>(ProcedureContext&, BytesSource)> handler);

// Get the number of registered procedure handlers
size_t GetProcedureHandlerCount();

// Helper to consume bytes from BytesSource (declared in Module.cpp)
std::vector<uint8_t> ConsumeBytes(BytesSource source);

// Forward declare the multiple primary key error function from Module.cpp
void SetMultiplePrimaryKeyError(const std::string& table_name);

// Forward declare the global V9 module accessor (defined in v9_builder.cpp)
RawModuleDefV9& GetV9Module();

// External global flags for circular reference detection (defined in v9_type_registration.cpp)
extern bool g_circular_ref_error;
extern std::string g_circular_ref_type_name;

/**
 * V9Builder - Builds a RawModuleDefV9 structure during module registration
 * 
 * This builder now uses the unified V9TypeRegistration system for all type handling.
 * It focuses solely on building tables, reducers, and module structure.
 * 
 * Type registration principles:
 * - Only user-defined structs/enums get registered (have entries in types array)
 * - Primitives, arrays, Options, special types are always inlined
 * - Single entry point for types: registerType() -> V9TypeRegistration
 */
class V9Builder {
public:
    V9Builder();
    
    /**
     * Register a type using the unified type registration system
     * Delegates to V9TypeRegistration::registerType()
     * 
     * @param bsatn_type The type to register
     * @param explicit_name Optional explicit name for the type
     * @param cpp_type Optional C++ type info
     * @return AlgebraicType - either inline or Ref to registered type
     */
    AlgebraicType registerType(const bsatn::AlgebraicType& bsatn_type,
                               const std::string& explicit_name = "",
                               const std::type_info* cpp_type = nullptr);
    
    /**
     * Register a table with all its constraints and metadata
     * This is the main entry point from SPACETIMEDB_TABLE macro
     * 
     * @tparam T The table struct type
     * @param table_name The name of the table
     * @param is_public Whether the table is public
     */
    template<typename T>
    void RegisterTable(const std::string& table_name, 
                       bool is_public);
    
    /**
     * Add a field constraint to a table after it has been registered
     * This is called by FIELD_ macros to add constraints separately
     * 
     * @tparam T The table struct type
     * @param table_name The name of the table
     * @param field_name The name of the field
     * @param constraint The constraint to add
     */
    template<typename T>
    void AddFieldConstraint(const std::string& table_name,
                           const std::string& field_name,
                           FieldConstraint constraint);
    
    /**
     * Add a multi-column index to a table after it has been registered
     * This is called by FIELD_NamedMultiColumnIndex macro
     * 
     * @tparam T The table struct type
     * @param table_name The name of the table
     * @param index_name The name of the index
     * @param field_names The field names in the index
     */
    template<typename T>
    void AddMultiColumnIndex(const std::string& table_name,
                            const std::string& index_name,
                            const std::vector<std::string>& field_names);
    
    /**
     * Add a column default value to a table after it has been registered
     * This is called by FIELD_Default macro
     * 
     * @tparam T The table struct type
     * @param table_name The name of the table
     * @param field_name The name of the field
     * @param serialized_value The BSATN-serialized default value
     */
    template<typename T>
    void AddColumnDefault(const std::string& table_name,
                         const std::string& field_name,
                         const std::vector<uint8_t>& serialized_value);
    
    /**
     * Register a reducer function with C++20 concepts
     * This is the main entry point from REGISTER_REDUCER macro
     * 
     * @tparam Func The reducer function type
     * @param reducer_name The name of the reducer
     * @param func The reducer function pointer
     */
    template<typename Func>
    void RegisterReducer(const std::string& reducer_name, Func func);
    
    /**
     * Register a reducer function with explicit parameter names
     * This overload is used when parameter names are available
     * 
     * @tparam Func The reducer function type
     * @param reducer_name The name of the reducer
     * @param func The reducer function pointer
     * @param param_names The names of the parameters (excluding ReducerContext)
     */
    template<typename Func>
    void RegisterReducer(const std::string& reducer_name, Func func,
                        const std::vector<std::string>& param_names);
    
    /**
     * Register a lifecycle reducer function
     * 
     * @tparam Func The reducer function type
     * @param reducer_name The name of the reducer
     * @param func The reducer function pointer
     * @param lifecycle The lifecycle type (Init, OnConnect, OnDisconnect)
     */
    template<typename Func>
    void RegisterLifecycleReducer(const std::string& reducer_name, Func func,
                                 Lifecycle lifecycle);
    
    /**
     * Register a view function
     * 
     * Views provide read-only query capabilities with caller-specific or anonymous contexts.
     * The template detects whether the view takes ViewContext or AnonymousViewContext.
     * 
     * @tparam Func The view function type (must take ViewContext or AnonymousViewContext)
     * @param view_name The name of the view
     * @param func The view function pointer
     * @param is_public Whether the view is publicly accessible
     * @param param_names The names of parameters (currently empty - parameters disabled)
     */
    template<typename Func>
    void RegisterView(const std::string& view_name, Func func,
                     bool is_public,
                     const std::vector<std::string>& param_names = {});
    
    /**
     * Register a procedure function
     * 
     * Procedures can return arbitrary values and perform computations.
     * 
     * Procedures are always public (no access control).
     * 
     * @tparam Func The procedure function type
     * @param procedure_name The name of the procedure
     * @param func The procedure function pointer
     * @param param_names The names of parameters
     */
    template<typename Func>
    void RegisterProcedure(const std::string& procedure_name,
                          Func func,
                          const std::vector<std::string>& param_names = {});
    
    /**
     * Register a schedule for a table to automatically call a reducer
     * when the scheduled_at field indicates it's time.
     * 
     * @param table_name The name of the table containing scheduled data
     * @param scheduled_at_column The column index of the ScheduleAt field (0-based)
     * @param reducer_name The name of the reducer to call
     */
    void RegisterSchedule(const std::string& table_name, 
                         uint16_t scheduled_at_column,
                         const std::string& reducer_name);
    
    /**
     * Register a row level security (RLS) policy for client visibility filtering.
     * These are collected and added to the module's row_level_security field.
     * 
     * @param sql_query The SQL query that defines the visibility filter
     */
    void RegisterRowLevelSecurity(const std::string& sql_query);
    
    /**
     * Add a complete V9 table definition with type registration and metadata.
     * This method handles the complete table addition including type registration.
     */
    void AddV9Table(const std::string& table_name,
                       const bsatn::AlgebraicType& table_type,
                       const std::type_info* cpp_type,
                       bool is_public,
                       const std::vector<uint16_t>& primary_key,
                       const std::vector<RawIndexDefV9>& indexes,
                       const std::vector<RawConstraintDefV9>& constraints,
                       const std::vector<RawSequenceDefV9>& sequences,
                       const std::optional<RawScheduleDefV9>& schedule);
    
    /**
     * Add a complete V9 reducer definition with parameter type registration.
     * This method handles the complete reducer addition including parameter type registration.
     */
    void AddV9Reducer(const std::string& reducer_name,
                         const std::vector<bsatn::AlgebraicType>& param_types,
                         const std::vector<std::string>& param_names,
                         const std::vector<const std::type_info*>& param_cpp_types,
                         const std::vector<std::string>& param_type_names,
                         std::optional<Lifecycle> lifecycle);
    
    /**
     * Serialize the module definition to binary.
     */
    std::vector<uint8_t> serialize() const;
    
private:
    // Store pending schedules to be applied when tables are registered
    struct PendingSchedule {
        std::string table_name;
        uint16_t scheduled_at_column;
        std::string reducer_name;
    };
    std::map<std::string, PendingSchedule> pending_schedules_;
    
    // Helper to find existing table by name in the module
    RawTableDefV9* findTableByName(const std::string& table_name);
    
    /**
     * Get field name from Product type structure.
     */
    std::string getFieldName(const bsatn::AlgebraicType& table_type, uint16_t column_index) const;
    
    /**
     * Generate constraints for primary key.
     */
    std::vector<RawConstraintDefV9> generateConstraintsForPrimaryKey(
        const std::string& table_name,
        const bsatn::AlgebraicType& table_type,
        const std::vector<uint16_t>& primary_key) const;
        
    /**
     * Generate indexes for primary key.
     */
    std::vector<RawIndexDefV9> generateIndexesForPrimaryKey(
        const std::string& table_name,
        const bsatn::AlgebraicType& table_type,
        const std::vector<uint16_t>& primary_key) const;
    
    /**
     * Helper to create a BTree index for a field
     */
    RawIndexDefV9 createBTreeIndex(const std::string& table_name,
                                   const std::string& field_name,
                                   uint16_t field_idx) const;
    
    /**
     * Helper to create a unique constraint for a field
     */
    RawConstraintDefV9 createUniqueConstraint(const std::string& table_name,
                                              const std::string& field_name,
                                              uint16_t field_idx) const;
    
    /**
     * @brief Common implementation for reducer registration
     * 
     * This helper eliminates ~80% code duplication between RegisterReducer and
     * RegisterLifecycleReducer by consolidating the common parameter extraction,
     * handler creation, and registration logic.
     * 
     * @tparam Func The function pointer type of the reducer
     * @param reducer_name Name of the reducer
     * @param func The reducer function
     * @param param_names Optional parameter names  
     * @param lifecycle Optional lifecycle type (nullopt for regular reducers)
     */
    template<typename Func>
    void RegisterReducerCommon(const std::string& reducer_name, 
                               Func func,
                               const std::vector<std::string>& param_names,
                               std::optional<Lifecycle> lifecycle);
};

// Template implementation for RegisterTable
template<typename T>
void V9Builder::RegisterTable(const std::string& table_name, 
                              bool is_public) {
    // RegisterTable implementation
    
    // First, ensure field registration happens
    SpacetimeDB::field_registrar<T>::register_fields();
    
    // Check if circular reference was detected during field registration
    if (g_circular_ref_error) {
        // Circular reference detected - don't register the table
        // preinit_99 will handle creating the error module
        fprintf(stdout, "DEBUG: Circular reference detected in table '%s', skipping registration\n", 
                table_name.c_str());
        return;
    }
    
    // Get field descriptors for the table type
    auto& descriptor_map = SpacetimeDB::get_table_descriptors();
    auto it = descriptor_map.find(&typeid(T));
    if (it == descriptor_map.end()) {
        // No descriptors registered for this type - this shouldn't happen if SPACETIMEDB_STRUCT was used
        return;
    }
    const auto& field_descs = it->second.fields;
    
    // Build a vector of BSATN ProductTypeElements
    std::vector<bsatn::ProductTypeElement> elements;
    
    // First collect all field types with their names and register enum types
    for (const auto& field_desc : field_descs) {
        bsatn::AlgebraicType field_type = field_desc.get_algebraic_type();
        std::string field_type_name = field_desc.get_type_name ? field_desc.get_type_name() : "";
        
        // For enum types (Sum types), register them by name first
        // This ensures they get proper type names in the V9 system
        if (!field_type_name.empty() && field_type.tag() == bsatn::AlgebraicTypeTag::Sum) {
            // Check if it's not an Option type (which should be inlined)
            const auto& sum = field_type.as_sum();
            bool is_option = (sum.variants.size() == 2 && 
                             sum.variants[0].name == "some" && 
                             sum.variants[1].name == "none");
            
            // Check if it's ScheduleAt (which should be inlined)
            bool is_schedule_at = (sum.variants.size() == 2 && 
                                  sum.variants[0].name == "Interval" && 
                                  sum.variants[1].name == "Time");
            
            // Check if it's a Result type (which should be inlined)
            bool is_result = (sum.variants.size() == 2 && 
                             sum.variants[0].name == "ok" && 
                             sum.variants[1].name == "err");
            
            if (!is_option && !is_schedule_at && !is_result) {
                // This is a user-defined enum, register it by name
                // Strip namespace from type name if present
                size_t last_colon = field_type_name.rfind("::");
                if (last_colon != std::string::npos) {
                    field_type_name = field_type_name.substr(last_colon + 2);
                }
                //fprintf(stdout, "DEBUG: Registering enum type '%s' for field '%s'\n", 
                //        field_type_name.c_str(), field_desc.name.c_str());
                getV9TypeRegistration().registerTypeByName(field_type_name, field_type, nullptr);
            }
        }
        
        elements.emplace_back(
            std::make_optional(field_desc.name),
            std::move(field_type)
        );
    }
    
    // Create the BSATN Product type with the elements
    bsatn::ProductType bsatn_product(std::move(elements));
    
    // Create the BSATN AlgebraicType with Product type
    bsatn::AlgebraicType table_type = bsatn::AlgebraicType::make_product(
        std::make_unique<bsatn::ProductType>(std::move(bsatn_product)));
    
    // Process constraints to create V9 structures
    std::vector<uint16_t> primary_key;
    std::vector<RawIndexDefV9> indexes;
    std::vector<RawConstraintDefV9> v9_constraints;
    std::vector<RawSequenceDefV9> sequences;
    
    // Constraints and indexes will be added later by FIELD_ macros via AddFieldConstraint
    // For now, start with empty vectors
    
    // Check if there's a pending schedule for this table
    std::optional<RawScheduleDefV9> schedule = std::nullopt;
    auto schedule_it = pending_schedules_.find(table_name);
    if (schedule_it != pending_schedules_.end()) {
        RawScheduleDefV9 schedule_def;
        schedule_def.scheduled_at_column = schedule_it->second.scheduled_at_column;
        schedule_def.reducer_name = schedule_it->second.reducer_name;
        schedule = schedule_def;
        
        // Remove the pending schedule since we've used it
        pending_schedules_.erase(schedule_it);
    }
    
    // Add the complete V9 table definition
    AddV9Table(table_name, table_type, &typeid(T), is_public,
               primary_key, indexes, v9_constraints, sequences, schedule);
}

// Template implementation for AddFieldConstraint
template<typename T>
void V9Builder::AddFieldConstraint(const std::string& table_name,
                                   const std::string& field_name,
                                   FieldConstraint constraint) {
    // AddFieldConstraint implementation
    
    // Find the existing table by name
    RawTableDefV9* table = findTableByName(table_name);
    if (!table) {
        fprintf(stderr, "ERROR: Table '%s' not found when trying to add constraint to field '%s'\n",
                table_name.c_str(), field_name.c_str());
        return;
    }
    
    // Get field descriptors to find the field index
    SpacetimeDB::field_registrar<T>::register_fields();
    auto& descriptor_map = SpacetimeDB::get_table_descriptors();
    auto it = descriptor_map.find(&typeid(T));
    if (it == descriptor_map.end()) {
        fprintf(stderr, "ERROR: No field descriptors found for table %s\n", table_name.c_str());
        return;
    }
    
    const auto& field_descs = it->second.fields;
    uint16_t field_idx = 0;
    bool field_found = false;
    
    // Find the field index
    for (const auto& field_desc : field_descs) {
        if (field_desc.name == field_name) {
            field_found = true;
            break;
        }
        field_idx++;
    }
    
    if (!field_found) {
        fprintf(stderr, "ERROR: Field '%s' not found in table '%s'\n", 
                field_name.c_str(), table_name.c_str());
        return;
    }
    
    // Add constraint based on type
    int constraint_bits = static_cast<int>(constraint);
    
    // Check for PrimaryKey (has specific bit 0b1000)
    if (constraint_bits & 0b1000) {  // PrimaryKey-specific bit
        // Validate that there isn't already a primary key
        if (!table->primary_key.empty()) {
            // Set the error flag instead of crashing - this will be handled by preinit_99
            SetMultiplePrimaryKeyError(table_name);
            return; // Exit early to avoid adding the conflicting primary key
        }
        table->primary_key.push_back(field_idx);
        
        // PrimaryKey implies Unique constraint and index
        table->constraints.push_back(createUniqueConstraint(table_name, field_name, field_idx));
        table->indexes.push_back(createBTreeIndex(table_name, field_name, field_idx));
        
        //fprintf(stdout, "DEBUG: Added PrimaryKey constraint and index for %s.%s\n", 
        //        table_name.c_str(), field_name.c_str());
    }
    // Check for Unique (has bit 0b0100, but not PrimaryKey)
    else if ((constraint_bits & 0b0100) && !(constraint_bits & 0b1000)) {
        table->constraints.push_back(createUniqueConstraint(table_name, field_name, field_idx));
        table->indexes.push_back(createBTreeIndex(table_name, field_name, field_idx));
        
        //fprintf(stdout, "DEBUG: Added Unique constraint and index for %s.%s\n", 
        //        table_name.c_str(), field_name.c_str());
    }
    // Check for plain Index (has bit 0b0001, but not Unique or PrimaryKey bits)
    else if ((constraint_bits & 0b0001) && !(constraint_bits & 0b1100)) {
        // Just create an index, no constraint
        table->indexes.push_back(createBTreeIndex(table_name, field_name, field_idx));
        
        //fprintf(stdout, "DEBUG: Added Index for %s.%s\n", 
        //        table_name.c_str(), field_name.c_str());
    }
    
    // Check for AutoInc
    if (constraint_bits & static_cast<int>(FieldConstraint::AutoInc)) {
        RawSequenceDefV9 seq_def;
        seq_def.name = table_name + "_" + field_name + "_seq";
        seq_def.column = field_idx;
        seq_def.start = std::nullopt;
        seq_def.increment = SpacetimeDB::I128(1);
        seq_def.min_value = std::nullopt;
        seq_def.max_value = std::nullopt;
        table->sequences.push_back(std::move(seq_def));
        
        //fprintf(stdout, "DEBUG: Added AutoInc sequence for %s.%s\n", 
        //        table_name.c_str(), field_name.c_str());
    }
}

// Template implementation for AddMultiColumnIndex
template<typename T>
void V9Builder::AddMultiColumnIndex(const std::string& table_name,
                                    const std::string& index_name,
                                    const std::vector<std::string>& field_names) {
    //fprintf(stdout, "DEBUG: Adding multi-column index '%s' to table '%s' with %zu fields\n", 
    //        index_name.c_str(), table_name.c_str(), field_names.size());
    
    // Find the existing table
    RawTableDefV9* table = findTableByName(table_name);
    if (!table) {
        fprintf(stderr, "ERROR: Table '%s' not found for multi-column index '%s'\n",
                table_name.c_str(), index_name.c_str());
        return;
    }
    
    // Get field descriptors to find the field index
    SpacetimeDB::field_registrar<T>::register_fields();
    auto& descriptor_map = SpacetimeDB::get_table_descriptors();
    auto it = descriptor_map.find(&typeid(T));
    if (it == descriptor_map.end()) {
        fprintf(stderr, "ERROR: No field descriptors found for table %s\n", table_name.c_str());
        return;
    }
    
    const auto& field_descs = it->second.fields;
    std::vector<uint16_t> field_indexes;
    
    // Find field indices for each field name
    for (const std::string& field_name : field_names) {
        uint16_t field_idx = 0;
        bool field_found = false;
        
        for (const auto& field_desc : field_descs) {
            if (field_desc.name == field_name) {
                field_indexes.push_back(field_idx);
                field_found = true;
                //fprintf(stdout, "DEBUG: Field '%s' -> index %u\n", field_name.c_str(), field_idx);
                break;
            }
            field_idx++;
        }
        
        if (!field_found) {
            fprintf(stderr, "ERROR: Field '%s' not found in table '%s' for multi-column index\n",
                    field_name.c_str(), table_name.c_str());
            return;
        }
    }
    
    // Create the multi-column BTree algorithm
    RawIndexAlgorithmBTreeData btree_data;
    btree_data.columns = field_indexes;
    
    RawIndexAlgorithm algorithm;
    algorithm.set<0>(btree_data);  // Set BTree variant
    
    // Create the index definition with both the user-provided name and generated btree name
    RawIndexDefV9 index_def;
    std::string generated_name = table_name + "_" + field_names[0];
    for (size_t i = 1; i < field_names.size(); ++i) {
        generated_name += "_" + field_names[i];
    }
    generated_name += "_idx_btree";  // Generated btree index name
    index_def.name = generated_name;
    index_def.accessor_name = index_name;  // User-provided index name for access
    index_def.algorithm = algorithm;
    
    // Add to table's indexes
    table->indexes.push_back(std::move(index_def));
    
    //fprintf(stdout, "DEBUG: Successfully added multi-column index '%s' -> '%s' with %zu fields\n",
    //        index_name.c_str(), generated_name.c_str(), field_indexes.size());
}

// Template implementation for AddColumnDefault
template<typename T>
void V9Builder::AddColumnDefault(const std::string& table_name,
                                 const std::string& field_name,
                                 const std::vector<uint8_t>& serialized_value) {
    // Find the existing table
    RawTableDefV9* table = findTableByName(table_name);
    if (!table) {
        fprintf(stderr, "ERROR: Table '%s' not found for default value on field '%s'\n",
                table_name.c_str(), field_name.c_str());
        return;
    }
    
    // Get field descriptors to find the field index
    SpacetimeDB::field_registrar<T>::register_fields();
    auto& descriptor_map = SpacetimeDB::get_table_descriptors();
    auto it = descriptor_map.find(&typeid(T));
    if (it == descriptor_map.end()) {
        fprintf(stderr, "ERROR: No field descriptors found for table %s\n", table_name.c_str());
        return;
    }
    
    const auto& field_descs = it->second.fields;
    uint16_t field_idx = 0;
    bool field_found = false;
    
    // Find the field index
    for (const auto& field_desc : field_descs) {
        if (field_desc.name == field_name) {
            field_found = true;
            break;
        }
        field_idx++;
    }
    
    if (!field_found) {
        fprintf(stderr, "ERROR: Field '%s' not found in table '%s'\n",
                field_name.c_str(), table_name.c_str());
        return;
    }
    
    // Validate: default values cannot be used with primary_key, unique, or auto_inc
    // Check if this column is in the primary key
    for (uint16_t pk_col : table->primary_key) {
        if (pk_col == field_idx) {
            std::string error_msg = "ERROR: Field " + table_name + "." + field_name + 
                        " has primary_key constraint - cannot have default value";
            fprintf(stderr, "%s", error_msg.c_str());
            // fprintf(stderr, "ERROR:  has primary_key constraint - cannot have default value",
            //          table_name.c_str(), field_name.c_str());
            return;
        }
    }
    
    // Check if this column has a unique constraint
    for (const auto& constraint : table->constraints) {
        if (constraint.data.get_tag() == 0) {  // Unique constraint variant
            const auto& unique_data = constraint.data.get<0>();
            if (unique_data.columns.size() == 1 && unique_data.columns[0] == field_idx) {
                fprintf(stderr, "ERROR: Field %s.%s has unique constraint - cannot have default value",
                        table_name.c_str(), field_name.c_str());
                return;
            }
        }
    }
    
    // Check if this column has an auto_inc sequence
    for (const auto& sequence : table->sequences) {
        if (sequence.column == field_idx) {
            fprintf(stderr, "ERROR: Field %s.%s has auto_inc constraint - cannot have default value",
                    table_name.c_str(), field_name.c_str());
            return;
        }
    }
    
    // Create the column default value structure
    RawColumnDefaultValueV9 col_default;
    col_default.table = table_name;
    col_default.col_id = field_idx;
    col_default.value = serialized_value;
    
    // Create the misc export entry with ColumnDefaultValue variant (variant 0)
    RawMiscModuleExportV9 export_entry;
    export_entry.set<0>(col_default);
    
    // Add to the module's misc_exports
    GetV9Module().misc_exports.push_back(export_entry);
}

// Helper trait to extract function parameter types
template<typename T>
struct function_traits;

template<typename R, typename... Args>
struct function_traits<R(*)(Args...)> {
    static constexpr size_t arity = sizeof...(Args);
    using result_type = R;
    
    template<size_t N>
    using arg_t = typename std::tuple_element<N, std::tuple<Args...>>::type;
};

// Helper to extract T from Outcome<T>
template<typename T>
struct outcome_inner_type;

template<typename T>
struct outcome_inner_type<Outcome<T>> {
    using type = T;
};


// Template implementation for RegisterReducerCommon - shared logic
template<typename Func>
void V9Builder::RegisterReducerCommon(const std::string& reducer_name, 
                                      Func func,
                                      const std::vector<std::string>& param_names,
                                      std::optional<Lifecycle> lifecycle) {
    // Skip reducer registration if circular reference was detected
    if (g_circular_ref_error) {
        fprintf(stdout, "DEBUG: Skipping reducer '%s' registration due to circular reference error\n", 
                reducer_name.c_str());
        return;
    }
    
    using traits = function_traits<Func>;
    
    // Validate that the reducer has at least one parameter (ReducerContext)
    static_assert(traits::arity > 0, 
        "Reducer must have at least one parameter (ReducerContext)");
    
    // Only validate the first parameter type if we have parameters
    // This prevents template instantiation errors when arity is 0
    if constexpr (traits::arity > 0) {
        using FirstParamType = std::remove_cv_t<std::remove_reference_t<
            typename traits::template arg_t<0>>>;
        
        static_assert(std::is_same_v<FirstParamType, ReducerContext>,
            "First parameter of reducer must be ReducerContext");
    }
    
    // Build vectors of parameter information
    std::vector<bsatn::AlgebraicType> param_types;
    std::vector<const std::type_info*> param_cpp_types;
    std::vector<std::string> param_type_names;
    
    // Extract parameter types (skip ReducerContext at index 0)
    if constexpr (traits::arity > 1) {
        []<std::size_t... Is>(std::index_sequence<Is...>, 
                              std::vector<bsatn::AlgebraicType>& types,
                              std::vector<const std::type_info*>& cpp_types) {
            (([]<std::size_t I>(std::vector<bsatn::AlgebraicType>& types_inner,
                                std::vector<const std::type_info*>& cpp_types_inner) {
                if constexpr (I > 0) {  // Skip the first parameter (ReducerContext)
                    using param_type = typename traits::template arg_t<I>;
                    types_inner.push_back(bsatn::bsatn_traits<param_type>::algebraic_type());
                    cpp_types_inner.push_back(&typeid(param_type));
                }
            }.template operator()<Is>(types, cpp_types)), ...);
        }(std::make_index_sequence<traits::arity>{}, param_types, param_cpp_types);
    }
    
    // Create the handler wrapper
    std::function<void(ReducerContext&, BytesSource)> handler;
    
    if constexpr (traits::arity == 1) {
        // Only ReducerContext parameter
        handler = [func](ReducerContext& ctx, BytesSource) {
            // Call the reducer and check the result
            auto result = func(ctx);
            if (result.is_err()) {
                // Reducer returned an error - store it for the caller
                fail_reducer(result.error());
            }
        };
    } else {
        // Has additional parameters
        handler = [func](ReducerContext& ctx, BytesSource args_source) {
            std::vector<uint8_t> args_bytes = ConsumeBytes(args_source);
            
            []<std::size_t... Js>(std::index_sequence<Js...>, 
                                  Func fn,
                                  ReducerContext& ctx_inner,
                                  const std::vector<uint8_t>& bytes) {
                if constexpr (sizeof...(Js) > 0) {
                    bsatn::Reader reader(bytes.data(), bytes.size());
                    auto args = std::make_tuple(
                        bsatn::deserialize<typename traits::template arg_t<Js + 1>>(reader)...
                    );
                    
                    std::apply([&ctx_inner, fn](auto&&... args) {
                        // Call the reducer and check the result
                        auto result = fn(ctx_inner, std::forward<decltype(args)>(args)...);
                        if (result.is_err()) {
                            // Reducer returned an error - store it for the caller
                            fail_reducer(result.error());
                        }
                    }, args);
                }
            }(std::make_index_sequence<traits::arity - 1>{}, func, ctx, args_bytes);
        };
    }
    
    // Generate parameter names if not provided
    std::vector<std::string> actual_param_names = param_names;
    if (actual_param_names.size() < param_types.size()) {
        actual_param_names.resize(param_types.size());
        for (size_t i = param_names.size(); i < param_types.size(); ++i) {
            actual_param_names[i] = "arg" + std::to_string(i);
        }
    }
    
    // Add the complete V9 reducer definition
    AddV9Reducer(reducer_name, param_types, actual_param_names, 
                 param_cpp_types, param_type_names, lifecycle);
    
    // Register the handler for runtime dispatch
    RegisterReducerHandler(reducer_name, handler, lifecycle);
}

// Template implementation for RegisterReducer using C++20 features
template<typename Func>
void V9Builder::RegisterReducer(const std::string& reducer_name, Func func) {
    // Call the overload with empty parameter names (for backwards compatibility)
    RegisterReducer(reducer_name, func, std::vector<std::string>{});
}

// Template implementation for RegisterReducer with explicit parameter names
template<typename Func>
void V9Builder::RegisterReducer(const std::string& reducer_name, Func func,
                                const std::vector<std::string>& param_names) {
    // Use the common helper function
    RegisterReducerCommon(reducer_name, func, param_names, std::nullopt);
}

// Template implementation for RegisterLifecycleReducer 
template<typename Func>
void V9Builder::RegisterLifecycleReducer(const std::string& reducer_name, Func func,
                                         Lifecycle lifecycle) {
    // Generate default parameter names and use the common helper
    std::vector<std::string> empty_names;
    RegisterReducerCommon(reducer_name, func, empty_names, lifecycle);
}

// Helper: Convert view return types to vector format (matching Rust's ViewReturn trait)
// Vec<T> stays as Vec<T>, Option<T> becomes Vec<T> with 0 or 1 elements
template<typename T>
std::vector<T> view_result_to_vec(std::vector<T>&& vec) {
    return std::move(vec);  // Already a vector
}

template<typename T>
std::vector<T> view_result_to_vec(const std::vector<T>& vec) {
    return vec;  // Already a vector
}

template<typename T>
std::vector<T> view_result_to_vec(std::optional<T>&& opt) {
    // Convert Option to Vec: Some(x) -> [x], None -> []
    std::vector<T> result;
    if (opt.has_value()) {
        result.push_back(std::move(*opt));
    }
    return result;
}

template<typename T>
std::vector<T> view_result_to_vec(const std::optional<T>& opt) {
    // Convert Option to Vec: Some(x) -> [x], None -> []
    std::vector<T> result;
    if (opt.has_value()) {
        result.push_back(*opt);
    }
    return result;
}

// Template implementation for RegisterView
template<typename Func>
void V9Builder::RegisterView(const std::string& view_name, Func func,
                             bool is_public,
                             const std::vector<std::string>& param_names) {
    // TODO: Remove this when parameters are supported - param_names will be used
    // Parameters are currently disabled - suppress warning
    (void)param_names;
    
    // Skip view registration if circular reference was detected
    if (g_circular_ref_error) {
        fprintf(stdout, "DEBUG: Skipping view '%s' registration due to circular reference error\n", 
                view_name.c_str());
        return;
    }
    
    using traits = function_traits<Func>;
    
    // Validate that the view has at least one parameter (ViewContext or AnonymousViewContext)
    static_assert(traits::arity > 0, 
        "View must have at least one parameter (ViewContext or AnonymousViewContext)");
    
    // Determine the context type and register accordingly
    if constexpr (traits::arity > 0) {
        using ContextType = std::remove_cv_t<std::remove_reference_t<
            typename traits::template arg_t<0>>>;
        
        // Extract return type
        using ReturnType = typename traits::result_type;
        
        // Build the AlgebraicType for the return type
        auto& type_reg = getV9TypeRegistration();
        bsatn::AlgebraicType bsatn_return_type = bsatn::algebraic_type_of<ReturnType>::get();
        AlgebraicType return_algebraic_type = type_reg.registerType(bsatn_return_type, "", &typeid(ReturnType));
        
        // TODO: When parameters are supported, extract parameter types and build ProductType:
        // Build params (empty for now since parameters are disabled)
        // std::vector<ProductTypeElement> param_elements;
        // if constexpr (traits::arity > 1) {
        //     []<std::size_t... Is>(std::index_sequence<Is...>, 
        //                           std::vector<ProductTypeElement>& elements,
        //                           const std::vector<std::string>& names,
        //                           V9TypeRegistration& type_reg_inner) {
        //         (([]<std::size_t I>(std::vector<ProductTypeElement>& elems,
        //                             const std::vector<std::string>& n,
        //                             V9TypeRegistration& tr) {
        //             if constexpr (I > 0) {  // Skip the first parameter (ViewContext/AnonymousViewContext)
        //                 using param_type = typename traits::template arg_t<I>;
        //                 bsatn::AlgebraicType param_bsatn = bsatn::algebraic_type_of<param_type>::get();
        //                 AlgebraicType param_alg = tr.registerType(param_bsatn, "", &typeid(param_type));
        //                 std::string param_name = (I-1 < n.size()) ? n[I-1] : ("arg" + std::to_string(I-1));
        //                 elems.emplace_back(std::make_optional(param_name), std::move(param_alg));
        //             }
        //         }.template operator()<Is>(elements, names, type_reg_inner)), ...);
        //     }(std::make_index_sequence<traits::arity>{}, param_elements, param_names, type_reg);
        // }
        // ProductType params(std::move(param_elements));
        ProductType params;
        
        if constexpr (std::is_same_v<ContextType, ViewContext>) {
            // Register with ViewContext (has sender)
            std::function<std::vector<uint8_t>(ViewContext&, BytesSource)> handler =
                [func](ViewContext& ctx, BytesSource args_source) -> std::vector<uint8_t> {
                    // TODO: When parameters are supported, deserialize args_source:
                    // For now, views don't have parameters (args_source is unused)
                    // std::vector<uint8_t> args_bytes = ConsumeBytes(args_source);
                    // bsatn::Reader reader(args_bytes.data(), args_bytes.size());
                    // auto args = std::make_tuple(
                    //     bsatn::deserialize<typename traits::template arg_t<1>>(reader),
                    //     bsatn::deserialize<typename traits::template arg_t<2>>(reader),
                    //     ...
                    // );
                    // auto result = std::apply([&ctx, func](auto&&... args) {
                    //     return func(ctx, std::forward<decltype(args)>(args)...);
                    // }, args);
                    (void)args_source;
                    
                    // Call the view function - returns raw type directly
                    auto result = func(ctx);
                    
                    // Convert result to vector format (Option<T> -> Vec<T>)
                    auto result_vec = view_result_to_vec(std::move(result));
                    
                    // Serialize using pooled buffer
                    IterBuf buf = IterBuf::take();
                    {
                        bsatn::Writer writer(buf.get());
                        bsatn::serialize(writer, result_vec);
                    }  // Destroy Writer before releasing buffer
                    return buf.release();
                };
            
            RegisterViewHandler(view_name, handler);
            
            // Add view definition to module's misc_exports
            uint32_t view_index = static_cast<uint32_t>(GetViewHandlerCount());
            RawViewDefV9 view_def{
                view_name,
                view_index - 1,  // Index is 0-based, we just added one
                is_public,  // is_public (from parameter)
                false,  // is_anonymous
                params,
                return_algebraic_type
            };
            RawMiscModuleExportV9 export_entry;
            export_entry.set<2>(view_def);  // Index 2 = View variant
            GetV9Module().misc_exports.push_back(export_entry);
            
        } else if constexpr (std::is_same_v<ContextType, AnonymousViewContext>) {
            // Register with AnonymousViewContext (no sender)
            std::function<std::vector<uint8_t>(AnonymousViewContext&, BytesSource)> handler =
                [func](AnonymousViewContext& ctx, BytesSource args_source) -> std::vector<uint8_t> {
                    // TODO: When parameters are supported, deserialize args_source (same pattern as ViewContext above)
                    // For now, views don't have parameters (args_source is unused)
                    (void)args_source;
                    
                    // Call the view function - returns raw type directly
                    auto result = func(ctx);
                    
                    // Convert result to vector format (Option<T> -> Vec<T>)
                    auto result_vec = view_result_to_vec(std::move(result));
                    
                    // Serialize using pooled buffer
                    IterBuf buf = IterBuf::take();
                    {
                        bsatn::Writer writer(buf.get());
                        bsatn::serialize(writer, result_vec);
                    }  // Destroy Writer before releasing buffer
                    return buf.release();
                };
            
            RegisterAnonymousViewHandler(view_name, handler);
            
            // Add view definition to module's misc_exports
            uint32_t view_index = static_cast<uint32_t>(GetAnonymousViewHandlerCount());
            RawViewDefV9 view_def{
                view_name,
                view_index - 1,  // Index is 0-based, we just added one
                is_public,  // is_public (from parameter)
                true,  // is_anonymous
                params,
                return_algebraic_type
            };
            RawMiscModuleExportV9 export_entry;
            export_entry.set<2>(view_def);  // Index 2 = View variant
            GetV9Module().misc_exports.push_back(export_entry);
            
        } else {
            static_assert(std::is_same_v<ContextType, ViewContext> || 
                         std::is_same_v<ContextType, AnonymousViewContext>,
                "First parameter of view must be ViewContext or AnonymousViewContext");
        }
    }
}

// Template implementation for RegisterProcedure
template<typename Func>
void V9Builder::RegisterProcedure(const std::string& procedure_name,
                                  Func func,
                                  const std::vector<std::string>& param_names) {
    // Skip procedure registration if circular reference was detected
    if (g_circular_ref_error) {
        fprintf(stdout, "DEBUG: Skipping procedure '%s' registration due to circular reference error\n", 
                procedure_name.c_str());
        return;
    }
    
    using traits = function_traits<Func>;
    
    // Validate that the procedure has at least one parameter (ProcedureContext)
    static_assert(traits::arity > 0, 
        "Procedure must have at least one parameter (ProcedureContext)");
    
    // Validate first parameter is ProcedureContext
    if constexpr (traits::arity > 0) {
        using FirstParamType = std::remove_cv_t<std::remove_reference_t<
            typename traits::template arg_t<0>>>;
        
        static_assert(std::is_same_v<FirstParamType, ProcedureContext>,
            "First parameter of procedure must be ProcedureContext");
    }
    
    // Procedures return raw T
    using ReturnType = typename traits::result_type;
    
    // Build the AlgebraicType for the return type
    auto& type_reg = getV9TypeRegistration();
    bsatn::AlgebraicType bsatn_return_type = bsatn::algebraic_type_of<ReturnType>::get();
    AlgebraicType return_algebraic_type = type_reg.registerType(bsatn_return_type, "", &typeid(ReturnType));
    
    // Build parameter types (skip ProcedureContext at index 0)
    std::vector<ProductTypeElement> param_elements;
    if constexpr (traits::arity > 1) {
        []<std::size_t... Is>(std::index_sequence<Is...>, 
                              std::vector<ProductTypeElement>& elements,
                              const std::vector<std::string>& names,
                              V9TypeRegistration& type_reg_inner) {
            (([]<std::size_t I>(std::vector<ProductTypeElement>& elems,
                                const std::vector<std::string>& n,
                                V9TypeRegistration& tr) {
                if constexpr (I > 0) {  // Skip the first parameter (ProcedureContext)
                    using param_type = typename traits::template arg_t<I>;
                    bsatn::AlgebraicType param_bsatn = bsatn::algebraic_type_of<param_type>::get();
                    AlgebraicType param_alg = tr.registerType(param_bsatn, "", &typeid(param_type));
                    std::string param_name = (I-1 < n.size()) ? n[I-1] : ("arg" + std::to_string(I-1));
                    elems.emplace_back(std::make_optional(param_name), std::move(param_alg));
                }
            }.template operator()<Is>(elements, names, type_reg_inner)), ...);
        }(std::make_index_sequence<traits::arity>{}, param_elements, param_names, type_reg);
    }
    ProductType params(std::move(param_elements));
    
    // Create handler that wraps the procedure function
    std::function<std::vector<uint8_t>(ProcedureContext&, BytesSource)> handler;
    
    if constexpr (traits::arity == 1) {
        // Only ProcedureContext parameter
        handler = [func](ProcedureContext& ctx, BytesSource) -> std::vector<uint8_t> {
            // Procedures return raw T
            // Use LOG_PANIC() for errors - procedures cannot return errors gracefully
            auto result = func(ctx);
            
            // Serialize using pooled buffer
            IterBuf buf = IterBuf::take();
            {
                bsatn::Writer writer(buf.get());
                bsatn::serialize(writer, result);
            }  // Destroy Writer before releasing buffer
            return buf.release();
        };
    } else {
        // Has additional parameters
        handler = [func](ProcedureContext& ctx, BytesSource args_source) -> std::vector<uint8_t> {
            std::vector<uint8_t> args_bytes = ConsumeBytes(args_source);
            
            return []<std::size_t... Js>(std::index_sequence<Js...>, 
                                        Func fn,
                                        ProcedureContext& ctx_inner,
                                        const std::vector<uint8_t>& bytes) -> std::vector<uint8_t> {
                if constexpr (sizeof...(Js) > 0) {
                    bsatn::Reader reader(bytes.data(), bytes.size());
                    auto args = std::make_tuple(
                        bsatn::deserialize<typename traits::template arg_t<Js + 1>>(reader)...
                    );
                    
                    // Procedures return raw T
                    // Use LOG_PANIC() for errors - procedures cannot return errors gracefully
                    auto result = std::apply([&ctx_inner, fn](auto&&... args) {
                        return fn(ctx_inner, std::forward<decltype(args)>(args)...);
                    }, args);
                    
                    // Serialize using pooled buffer
                    IterBuf buf = IterBuf::take();
                    {
                        bsatn::Writer writer(buf.get());
                        bsatn::serialize(writer, result);
                    }  // Destroy Writer before releasing buffer
                    return buf.release();
                } else {
                    return std::vector<uint8_t>();
                }
            }(std::make_index_sequence<traits::arity - 1>{}, func, ctx, args_bytes);
        };
    }
    
    RegisterProcedureHandler(procedure_name, handler);
    
    // Add procedure definition to module's misc_exports
    RawProcedureDefV9 procedure_def{
        procedure_name,
        params,
        return_algebraic_type
    };
    RawMiscModuleExportV9 export_entry;
    export_entry.set<1>(procedure_def);  // Index 1 = Procedure variant (0=ColumnDefaultValue, 1=Procedure, 2=View)
    GetV9Module().misc_exports.push_back(export_entry);
}

// Global V9Builder instance for the module
extern std::unique_ptr<V9Builder> g_v9_builder;

// Initialize the V9 builder (called once at module startup)
void initializeV9Builder();

// Get the global V9 builder
V9Builder& getV9Builder();

// =============================================================================
// Helper Functions (used internally by RegisterTable and RegisterReducer)
// =============================================================================

} // namespace Internal
} // namespace SpacetimeDB

#endif // SPACETIMEDB_V9_BUILDER_H