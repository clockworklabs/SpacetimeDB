#ifndef SPACETIMEDB_MODULE_H
#define SPACETIMEDB_MODULE_H

#include <string>
#include <vector>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <typeinfo>
#include "../bsatn/types.h"
#include "../reducer_macros.h"
#include "../reducer_context.h"
#include "../abi/opaque_types.h"
#include <spacetimedb/abi/FFI.h>  // For StatusCode
#include "autogen/TableAccess.g.h"  // For TableAccess enum
#include "autogen/Lifecycle.g.h"  // For Lifecycle enum
#include "autogen/RawTypeDefV9.g.h"  // For RawTypeDefV9
#include "autogen/RawModuleDefV9.g.h"  // For RawModuleDefV9

namespace SpacetimeDB {
struct FieldConstraintInfo;  // Forward declaration for constraint info

namespace bsatn {
class Writer;  // Forward declaration
}

namespace Internal {
struct RawScheduleDefV9;  // Forward declaration
struct RawModuleDefV9;    // Forward declaration
}

namespace Internal {

// Forward declarations are handled by includes

// FieldInfo for table registration
struct FieldInfo {
    const char* name;
    uint8_t type_id;
    size_t offset;
    size_t size;
    std::function<void(std::vector<uint8_t>&, const void*)> serialize;
};

// Raw module definition structure (similar to C# RawModuleDefV9)
struct RawModuleDef {
    // Structure to store named index information
    struct IndexInfo {
        std::string name;           // Index name (e.g., "foo" for test_a)
        std::string accessor_name;  // Accessor name for the index
        std::vector<uint16_t> columns;  // Column indices
    };
    
    struct Table {
        std::string name;
        bool is_public;
        const std::type_info* type;
        std::vector<FieldInfo> fields;
        std::function<void(std::vector<uint8_t>&)> write_schema;
        std::function<void(std::vector<uint8_t>&, const void*)> serialize;
        
        // Constraint metadata
        std::optional<uint16_t> primary_key;
        std::vector<uint16_t> unique_columns;
        std::vector<uint16_t> indexed_columns;
        std::vector<uint16_t> autoinc_columns;
        
        // Named indexes (for NamedIndex macro support)
        std::vector<IndexInfo> named_indexes;
        
        // Scheduled reducer metadata (pointer to avoid incomplete type issues)
        SpacetimeDB::Internal::RawScheduleDefV9* schedule = nullptr;
        
        // Field type collection migrated to V9Builder system
    };
    
    struct Reducer {
        std::string name;
        std::function<void(std::vector<uint8_t>&)> write_params;
        // Legacy write_params_with_registry removed - V9 system handles parameters
        std::function<void(ReducerContext&, uint32_t)> handler;
        std::optional<Lifecycle> lifecycle;
        // Parameter type collection migrated to V9Builder system
        
        // V9 support: Store parameter metadata for RawReducerDefV9
        std::vector<std::string> param_names;  // Parameter names from macro
    };
    
    std::vector<Table> tables;
    std::vector<Reducer> reducers;
    std::vector<RawTypeDefV9> types;
    std::map<const std::type_info*, size_t> table_indices;
    
    // V9 ModuleDef built incrementally during registration
    mutable RawModuleDefV9 v9_module;
    
    // Direct V9 type registration - replaces TypeRegistry
    // Returns the typespace index for the type
    uint32_t registerOrLookupType(const bsatn::AlgebraicType& type, 
                                  const std::string& type_name = "",
                                  bool is_table_type = false) const;
    
    // Helper to convert bsatn types to Internal types with proper references
    SpacetimeDB::Internal::AlgebraicType convertWithReferences(const bsatn::AlgebraicType& type) const;
    
    void AddTable(Table table) {
        table_indices[table.type] = tables.size();
        tables.push_back(std::move(table));
    }
    
    void AddReducer(Reducer reducer) {
        reducers.push_back(std::move(reducer));
    }
    
    // Serialize the entire module definition to binary format
    std::vector<uint8_t> serialize() const;
    
    // Legacy serialization method (manual BSATN writing)
    std::vector<uint8_t> serialize_legacy() const;

};

// Module class - singleton pattern similar to C# static class
class Module {
private:
    RawModuleDef module_def_;
    
    Module() = default;
    Module(const Module&) = delete;
    Module& operator=(const Module&) = delete;
    
public:
    static Module& Instance() {
        static Module instance;
        return instance;
    }
    
    // Get module definition (for internal use)
    static RawModuleDef& GetModuleDef() {
        return Instance().module_def_;
    }
    
    // Initialize module (called once)
    
    // Module description for FFI (matching existing signature)
    static void __describe_module__(BytesSink sink);
    
    // Reducer invocation for FFI (matching existing signature)
    static Status __call_reducer__(
        uint32_t id,
        uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
        uint64_t conn_id_0, uint64_t conn_id_1,
        Timestamp timestamp,
        BytesSource args_source,
        BytesSink error_sink
    );
    
    // View invocation for FFI (with sender)
    static int16_t __call_view__(
        uint32_t id,
        uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
        BytesSource args_source,
        BytesSink result_sink
    );
    
    // View invocation for FFI (anonymous - no sender)
    static int16_t __call_view_anon__(
        uint32_t id,
        BytesSource args_source,
        BytesSink result_sink
    );
    
    // Procedure invocation for FFI
    static int16_t __call_procedure__(
        uint32_t id,
        uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
        uint64_t conn_id_0, uint64_t conn_id_1,
        uint64_t timestamp_microseconds,
        BytesSource args_source,
        BytesSink result_sink
    );
    
    // Internal registration methods (inline to avoid linking issues)
    template<typename T>
    static void RegisterTableInternal(const char* name, bool is_public) {
        // Forward declaration - implementation will be included at end of file
        RegisterTableInternalImpl<T>(name, is_public);
    }
    
    template<typename... Args>
    static void RegisterReducerInternal(const std::string& name, void (*func)(ReducerContext, Args...)) {
        // Forward declaration - implementation will be included at end of file  
        RegisterReducerInternalImpl<Args...>(name, func);
    }
    
    // Implementation methods (will be defined after including Module_impl.h)
    template<typename T>
    static void RegisterTableInternalImpl(const char* name, bool is_public);
    
public:
    // New overload that accepts constraints - must be public for table_with_constraints.h
    template<typename T>
    static void RegisterTableInternalImpl(const char* name, bool is_public, 
                                         const std::vector<FieldConstraintInfo>& constraints);
    
private:
    
public:
    // These need to be public for macro access
    template<typename... Args>
    static void RegisterReducerInternalImpl(const std::string& name, void (*func)(ReducerContext, Args...));
    
    template<typename... Args>
    static void RegisterReducerInternalWithNames(const std::string& name, void (*func)(ReducerContext, Args...), const std::vector<std::string>& param_names);
    
private:
    
public:
    // Direct table registration (for rust_style_table.h)
    static void RegisterTableDirect(const std::string& name, 
                                   TableAccess access,
                                   std::function<std::vector<uint8_t>()> typeGen);
    
    // Special registration for lifecycle reducers
    static void RegisterInitReducer(void (*func)(ReducerContext));
    static void RegisterClientConnectedReducer(void (*func)(ReducerContext, Identity));
    static void RegisterClientDisconnectedReducer(void (*func)(ReducerContext, Identity));
    
public:
    // Registration support migrated to V9Builder system
};

// Helper functions for module description
std::vector<uint8_t> ConsumeBytes(BytesSource source);
void WriteBytes(BytesSink sink, const std::vector<uint8_t>& bytes);

// Schedule registration function
void register_table_schedule(const char* table_name, uint16_t scheduled_at_column, const char* reducer_name);

// Get the global V9 module for direct population
RawModuleDefV9& GetV9Module();

} // namespace Internal

// Public API similar to C# Module class
class Module {
public:
    // Table registration
    template<typename T>
    static void RegisterTable(const char* name, bool is_public = true) {
        Internal::Module::RegisterTableInternal<T>(name, is_public);
    }
    
    // Reducer registration
    template<typename... Args>
    static void RegisterReducer(const char* name, void (*func)(ReducerContext&, Args...)) {
        Internal::Module::RegisterReducerInternal(name, func);
    }
    
    // Client visibility filter (similar to C#)
    static void RegisterClientVisibilityFilter([[maybe_unused]] const char* sql) {
        // TODO: Implement when row-level security is added
    }
    
    // Module metadata (future extension)
    static void SetMetadata([[maybe_unused]] const char* name, [[maybe_unused]] const char* version) {
        // TODO: Implement module metadata
    }
};

// Global registration functions for X-Macro support
template<typename T>
void register_table_impl(const char* name, bool is_public) {
    Internal::Module::RegisterTableInternal<T>(name, is_public);
}

template<typename... Args>
void register_reducer_impl(const std::string& name, void (*func)(ReducerContext, Args...)) {
    Internal::Module::RegisterReducerInternal(name, func);
}

// Initialize module - no longer needed in V9 as preinit functions handle everything
inline void initialize_module() {
    // No-op in V9
}

// Write module definition (for FFI)
inline void spacetimedb_write_module_def(uint32_t sink) {
    BytesSink bs{sink};
    Internal::Module::__describe_module__(bs);
}

// Call reducer (for FFI)
inline int16_t spacetimedb_call_reducer(uint32_t id, uint32_t args, 
                                       uint64_t sender_0, uint64_t sender_1, 
                                       uint64_t sender_2, uint64_t sender_3) {
    // Create a simple timestamp
    Timestamp ts(0);
    BytesSource args_source{args};
    BytesSink error_sink{0};  // Null sink for now
    
    auto status = Internal::Module::__call_reducer__(
        id, sender_0, sender_1, sender_2, sender_3, 
        0, 0, ts, args_source, error_sink);
    
    return is_ok(status) ? 0 : -1;
}

} // namespace SpacetimeDB

// Include the template implementations
#include "Module_impl.h"

#endif // SPACETIMEDB_MODULE_H