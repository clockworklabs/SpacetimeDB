#ifndef SPACETIMEDB_MODULE_H
#define SPACETIMEDB_MODULE_H

#include <string>
#include <vector>
#include <optional>
#include "../bsatn/types.h"
#include "../reducer_macros.h"
#include "../reducer_context.h"
#include "../abi/opaque_types.h"
#include <spacetimedb/abi/FFI.h>  // For StatusCode
#include "autogen/TableAccess.g.h"  // For TableAccess enum
#include "autogen/Lifecycle.g.h"  // For Lifecycle enum
#include "autogen/CaseConversionPolicy.g.h"  // For CaseConversionPolicy

namespace SpacetimeDB {
namespace bsatn {
class Writer;  // Forward declaration
}

namespace Internal {

// Forward declarations are handled by includes

// Module class - singleton pattern similar to C# static class
class Module {
private:
    Module() = default;
    Module(const Module&) = delete;
    Module& operator=(const Module&) = delete;
    
public:
    static Module& Instance() {
        static Module instance;
        return instance;
    }
    
    // Initialize module (called once)
    
    // Module description for FFI (matching existing signature)
    static void __describe_module__(BytesSink sink);
    static std::vector<uint8_t> SerializeModuleDef();
    
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
    static void RegisterTableInternal(const char* name, bool is_public, bool is_event = false) {
        // Forward declaration - implementation will be included at end of file
        RegisterTableInternalImpl<T>(name, is_public, is_event);
    }
    
    template<typename Func>
    static void RegisterReducerInternal(const std::string& name, Func func) {
        // Forward declaration - implementation will be included at end of file  
        RegisterReducerInternalImpl<Func>(name, func);
    }
    
    // Implementation methods (will be defined after including Module_impl.h)
    template<typename T>
    static void RegisterTableInternalImpl(const char* name, bool is_public, bool is_event = false);

public:
    // These need to be public for macro access
    template<typename Func>
    static void RegisterReducerInternalImpl(const std::string& name, Func func);
    
    template<typename Func>
    static void RegisterReducerInternalWithNames(const std::string& name, Func func, const std::vector<std::string>& param_names);
    
private:
    
public:
    // Registration support routed through the V10 module-definition builder.
    static void RegisterClientVisibilityFilter(const char* sql);
    static void SetCaseConversionPolicy(CaseConversionPolicy policy);
    static void RegisterExplicitTableName(const std::string& source_name, const std::string& canonical_name);
    static void RegisterExplicitFunctionName(const std::string& source_name, const std::string& canonical_name);
    static void RegisterExplicitIndexName(const std::string& source_name, const std::string& canonical_name);
};

// Helper functions for module description
std::vector<uint8_t> ConsumeBytes(BytesSource source);
void WriteBytes(BytesSink sink, const std::vector<uint8_t>& bytes);

void SetTableIsEventFlag(const std::string& table_name, bool is_event);
bool GetTableIsEventFlag(const std::string& table_name);

} // namespace Internal

// Public alias to mirror C# API shape (`SpacetimeDB.CaseConversionPolicy`).
using CaseConversionPolicy = Internal::CaseConversionPolicy;

// Public API similar to C# Module class
class Module {
public:
    // Table registration
    template<typename T>
    static void RegisterTable(const char* name, bool is_public = true, bool is_event = false) {
        Internal::Module::RegisterTableInternal<T>(name, is_public, is_event);
    }
    
    // Reducer registration
    template<typename Func>
    static void RegisterReducer(const char* name, Func func) {
        Internal::Module::RegisterReducerInternal(name, func);
    }
    
    // Client visibility filter (similar to C# / Rust)
    static void RegisterClientVisibilityFilter(const char* sql) {
        Internal::Module::RegisterClientVisibilityFilter(sql);
    }
    
    // Module metadata (future extension)
    static void SetMetadata([[maybe_unused]] const char* name, [[maybe_unused]] const char* version) {
        // TODO: Implement module metadata
    }

    static void SetCaseConversionPolicy(CaseConversionPolicy policy) {
        Internal::Module::SetCaseConversionPolicy(policy);
    }

    static void RegisterExplicitTableName(const char* source_name, const char* canonical_name) {
        Internal::Module::RegisterExplicitTableName(source_name, canonical_name);
    }

    static void RegisterExplicitFunctionName(const char* source_name, const char* canonical_name) {
        Internal::Module::RegisterExplicitFunctionName(source_name, canonical_name);
    }

    static void RegisterExplicitIndexName(const char* source_name, const char* canonical_name) {
        Internal::Module::RegisterExplicitIndexName(source_name, canonical_name);
    }
};

// Global registration functions for X-Macro support
template<typename T>
void register_table_impl(const char* name, bool is_public) {
    Internal::Module::RegisterTableInternal<T>(name, is_public);
}

template<typename Func>
void register_reducer_impl(const std::string& name, Func func) {
    Internal::Module::RegisterReducerInternal(name, func);
}

// Initialize module - no-op; preinit functions handle registration.
inline void initialize_module() {
    // No-op.
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
