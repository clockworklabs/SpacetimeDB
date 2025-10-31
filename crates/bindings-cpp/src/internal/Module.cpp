// SpacetimeDB C++ SDK - V9 Serialization Implementation
// Clean implementation for simplified type registration and serialization

#include "spacetimedb.h"
#include "spacetimedb/internal/Module.h"
#include "spacetimedb/internal/autogen/RawModuleDefV9.g.h"
#include "spacetimedb/internal/bsatn_adapters.h"
#include "spacetimedb/internal/v9_type_registration.h"
#include "spacetimedb/abi/FFI.h"
#include "spacetimedb/bsatn/bsatn.h"
#include "spacetimedb/reducer_error.h"
#include <cstring>
#include <iostream>
#include <vector>
#include <functional>
#include <cctype>

namespace SpacetimeDb {

// Global V9 module structure - accessible from preinit functions
namespace Internal {
    // Thread-local reducer error message storage
    thread_local std::optional<std::string> g_reducer_error_message = std::nullopt;

    // The global V9 module that preinit functions will populate directly
    static RawModuleDefV9 g_v9_module;
    
    // Global reducer handler storage for runtime dispatch
    struct ReducerHandler {
        std::string name;
        std::function<void(ReducerContext&, BytesSource)> handler;
        std::optional<Lifecycle> lifecycle;
    };
    static std::vector<ReducerHandler> g_reducer_handlers;
    
    // Global error flag for multiple primary key detection
    static bool g_multiple_primary_key_error = false;
    static std::string g_multiple_primary_key_table_name = "";
    
    // External global flags for circular reference detection (defined in v9_type_registration.cpp)
    extern bool g_circular_ref_error;
    extern std::string g_circular_ref_type_name;
    
    // Function to set the multiple primary key error flag
    void SetMultiplePrimaryKeyError(const std::string& table_name) {
        g_multiple_primary_key_error = true;
        g_multiple_primary_key_table_name = table_name;
        fprintf(stderr, "ERROR: Multiple primary keys detected in table '%s'\n", table_name.c_str());
    }
    
    // Register a reducer handler (called by V9Builder during registration)
    void RegisterReducerHandler(const std::string& name, 
                                std::function<void(ReducerContext&, BytesSource)> handler,
                                std::optional<Lifecycle> lifecycle) {
        g_reducer_handlers.push_back({name, handler, lifecycle});
    }
    
    // Get the global V9 module
    RawModuleDefV9& GetV9Module() {
        return g_v9_module;
    }
    
    // Clear the global V9 module state - called at module initialization
    void ClearV9Module() {
        g_v9_module = RawModuleDefV9{};  // Reset to default state
        g_reducer_handlers.clear();  // Also clear reducer handlers
        g_multiple_primary_key_error = false;  // Reset error flag
        g_multiple_primary_key_table_name = "";  // Reset error table name
    }
    
    
    // Legacy DeferredRegistry system removed - all registration now handled by V9Builder

// Clear global state preinit function - runs before anything else
// The number 01 ensures this runs first to clear any leftover state
extern "C" __attribute__((export_name("__preinit__01_clear_global_state")))
void __preinit__01_clear_global_state() {
    ClearV9Module();
    // Also clear the V9 type registration state
    auto& type_reg = getV9TypeRegistration();
    type_reg.clear();
}

// Validation preinit function - runs after tables (20) and reducers (30)
// The number 99 ensures this runs last, just before __describe_module__
extern "C" __attribute__((export_name("__preinit__99_validate_types")))
void __preinit__99_validate_types() {
    fprintf(stdout, "\n[PREINIT_99] Starting validation\n");
    fprintf(stdout, "  g_circular_ref_error = %s\n", g_circular_ref_error ? "true" : "false");
    if (g_circular_ref_error) {
        fprintf(stdout, "  g_circular_ref_type_name = %s\n", g_circular_ref_type_name.c_str());
    }
    fflush(stdout);
    
    // Check if circular reference error occurred during type building
    if (g_circular_ref_error) {
        // Circular reference error detected - create a special error module
        
        // Clear the entire V9 module to start fresh
        RawModuleDefV9& v9_module = GetV9Module();
        v9_module.typespace.types.clear();
        v9_module.types.clear();
        v9_module.tables.clear();
        v9_module.reducers.clear();
        v9_module.misc_exports.clear();
        
        // Also clear the V9 type registration to remove any partial registrations
        auto& type_reg = getV9TypeRegistration();
        type_reg.clear();
        
        // Create the error type name that indicates the circular reference
        std::string error_type_name = "ERROR_CIRCULAR_REFERENCE_" + g_circular_ref_type_name;
        
        // Add a single named type export that points to a non-existent typespace index
        // This will cause SpacetimeDB to error when it tries to resolve the type
        RawTypeDefV9 error_type;
        error_type.name.scope = {};
        error_type.name.name = error_type_name;
        error_type.ty = 999999; // Invalid typespace index - will cause an error
        error_type.custom_ordering = false;
        
        v9_module.types.push_back(error_type);
        
        // Don't add anything to the typespace - this ensures the reference is invalid
        // The server will fail with an error message that includes our error type name
        
        // Also log to stderr for debugging
        fprintf(stderr, "\n[CIRCULAR REFERENCE ERROR] Module cleared and replaced with error type: %s\n", error_type_name.c_str());
        fprintf(stderr, "  Type '%s' contains a circular reference to itself\n", g_circular_ref_type_name.c_str());
        fflush(stderr);
        return; // Exit early, don't check other errors
    }
    
    // Check if multiple primary key error occurred during constraint registration
    if (g_multiple_primary_key_error) {
        // Multiple primary key error detected - create a special error module
        
        // Clear the entire V9 module to start fresh
        RawModuleDefV9& v9_module = GetV9Module();
        v9_module.typespace.types.clear();
        v9_module.types.clear();
        v9_module.tables.clear();
        v9_module.reducers.clear();
        v9_module.misc_exports.clear();
        
        // Create the error type name
        std::string error_type_name = "ERROR_MULTIPLE_PRIMARY_KEYS_" + g_multiple_primary_key_table_name;
        
        // Add a single named type export that points to a non-existent typespace index
        // This will cause SpacetimeDB to error when it tries to resolve the type
        RawTypeDefV9 error_type;
        error_type.name.scope = {};
        error_type.name.name = error_type_name;
        error_type.ty = 999999; // Invalid typespace index - will cause an error
        error_type.custom_ordering = false;
        
        v9_module.types.push_back(error_type);
        
        // Don't add anything to the typespace - this ensures the reference is invalid
        // The server will fail with an error message that includes our error type name
        
        // Also log to stderr for debugging
        fprintf(stderr, "\n[CONSTRAINT ERROR] Module cleared and replaced with error type: %s\n", error_type_name.c_str());
        fprintf(stderr, "Original error: Multiple primary keys detected in table '%s'\n\n", g_multiple_primary_key_table_name.c_str());
        fflush(stderr);
        
        return; // Exit early, don't check type registration errors
    }
    
    // Check if any errors occurred during type registration
    auto& type_reg = getV9TypeRegistration();
    if (type_reg.hasError()) {
        // Type registration detected an error - create a special error module
        const std::string& error = type_reg.getErrorMessage();
        
        // Clear the entire V9 module to start fresh
        RawModuleDefV9& v9_module = GetV9Module();
        v9_module.typespace.types.clear();
        v9_module.types.clear();
        v9_module.tables.clear();
        v9_module.reducers.clear();
        v9_module.misc_exports.clear();
        
        // Create an error type name that embeds the error message and type structure
        std::string error_type_name;
        if (error.find("Recursive type reference") != std::string::npos) {
            // Extract the type name from the error message
            size_t start = error.find("'");
            size_t end = error.rfind("'");
            std::string problematic_type = "unknown";
            if (start != std::string::npos && end != std::string::npos && end > start) {
                problematic_type = error.substr(start + 1, end - start - 1);
            }
            error_type_name = "ERROR_RECURSIVE_TYPE_" + problematic_type;
        } else if (error.find("Missing type name") != std::string::npos) {
            // Get the type description and sanitize it for use in the error name
            std::string type_desc = type_reg.getErrorTypeDescription();
            
            // Replace problematic characters with underscores
            for (char& c : type_desc) {
                if (!std::isalnum(c) && c != '_') {
                    c = '_';
                }
            }
            
            // Limit length to avoid overly long names
            if (type_desc.length() > 100) {
                type_desc = type_desc.substr(0, 100);
            }
            
            error_type_name = "ERROR_MISSING_TYPE_NAME_" + type_desc;
        } else {
            error_type_name = "ERROR_TYPE_REGISTRATION_FAILED";
        }
        
        // Add a single named type export that points to a non-existent typespace index
        // This will cause SpacetimeDB to error when it tries to resolve the type
        RawTypeDefV9 error_type;
        error_type.name.scope = {};
        error_type.name.name = error_type_name;
        error_type.ty = 999999; // Invalid typespace index - will cause an error
        error_type.custom_ordering = false;
        
        v9_module.types.push_back(error_type);
        
        // Don't add anything to the typespace - this ensures the reference is invalid
        // The server will fail with an error message that includes our error type name
        
        // Also log to stderr for debugging
        fprintf(stderr, "\n[TYPE ERROR] Module cleared and replaced with error type: %s\n", error_type_name.c_str());
        fprintf(stderr, "Original error: %s\n\n", error.c_str());
        fflush(stderr);
    }
    
    // Type validation passed - log statistics only in debug mode
    #ifdef DEBUG_TYPE_REGISTRATION
    else {
        RawModuleDefV9& v9_module = GetV9Module();
        fprintf(stderr, "[Type Validation] OK - %zu types, %zu tables, %zu reducers\n",
                v9_module.typespace.types.size(),
                v9_module.tables.size(), 
                v9_module.reducers.size());
    }
    #endif
}


// FFI export - V9 serialization
void Internal::Module::__describe_module__(BytesSink sink) {
    // The preinit functions should have already been called by SpacetimeDB
    // Including our validation preinit which checks for recursive types

    // Get the global V9 module
    RawModuleDefV9& v9_module = GetV9Module();
    
    
    // Create a buffer and writer
    std::vector<uint8_t> buffer;
    bsatn::Writer writer(buffer); 
    // Write version byte
    writer.write_u8(1);
    
    // Serialize the V9 module with our manually added table
    v9_module.bsatn_serialize(writer);
    
    // Now try to write using FFI directly
    if (!buffer.empty()) {
        size_t bytes_to_write = buffer.size();
        FFI::bytes_sink_write(sink, buffer.data(), &bytes_to_write);
    }
}

// Helper to consume all bytes from a BytesSource
std::vector<uint8_t> ConsumeBytes(BytesSource source) {
    if (source.inner == 0) {
        return {};
    }
    
    std::vector<uint8_t> buffer;
    constexpr size_t CHUNK_SIZE = 1024;
    buffer.reserve(CHUNK_SIZE);
    
    while (true) {
        size_t chunk_size = CHUNK_SIZE;
        size_t old_size = buffer.size();
        buffer.resize(old_size + chunk_size);
        
        auto ret = FFI::bytes_source_read(source, buffer.data() + old_size, &chunk_size);
        buffer.resize(old_size + chunk_size);  // Resize to actual bytes read
        
        if (ret == -1) {  // EXHAUSTED
            break;
        } else if (ret != 0) {  // Error
            fprintf(stderr, "ERROR: Failed to read from BytesSource: %d\n", ret);
            break;
        }
    }
    
    return buffer;
}

// Helper to write bytes to a BytesSink
void WriteBytes(BytesSink sink, const std::vector<uint8_t>& bytes) {
    if (sink.inner == 0 || bytes.empty()) {
        return;
    }
    
    size_t bytes_to_write = bytes.size();
    FFI::bytes_sink_write(sink, bytes.data(), &bytes_to_write);
}

Status Module::__call_reducer__(
    uint32_t id,
    uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
    uint64_t conn_id_0, uint64_t conn_id_1,
    Timestamp timestamp,
    BytesSource args_source,
    BytesSink error_sink
) {
    // Clear any previous error state
    SpacetimeDb::Internal::clear_reducer_error();

    // Check if reducer ID is valid
    if (id >= g_reducer_handlers.size()) {
        fprintf(stderr, "ERROR: Invalid reducer ID %u (have %zu reducers)\n", 
                id, g_reducer_handlers.size());
        
        // Write error message
        std::string error = "Invalid reducer ID: " + std::to_string(id);
        WriteBytes(error_sink, std::vector<uint8_t>(error.begin(), error.end()));
        return StatusCode::NO_SUCH_REDUCER;
    }
    
    // Create reducer context
    std::array<uint8_t, 32> sender_bytes{};
    // Pack the 4 uint64_t parts into 32 bytes
    std::memcpy(sender_bytes.data(), &sender_0, 8);
    std::memcpy(sender_bytes.data() + 8, &sender_1, 8);
    std::memcpy(sender_bytes.data() + 16, &sender_2, 8);
    std::memcpy(sender_bytes.data() + 24, &sender_3, 8);
    
    Identity sender_identity(sender_bytes);
    
    // Create connection ID if provided
    std::optional<ConnectionId> connection_id;
    if (conn_id_0 != 0 || conn_id_1 != 0) {
        // ConnectionId is 128-bit (two 64-bit parts)
        connection_id = ConnectionId(u128(conn_id_1, conn_id_0));
    }
    
    ReducerContext ctx(sender_identity, connection_id, timestamp);
    
    // Get the handler
    const auto& handler_info = g_reducer_handlers[id];
    
    // Call the reducer handler
    handler_info.handler(ctx, args_source);
    if (SpacetimeDb::Internal::has_reducer_error()) {
        std::string error_msg = SpacetimeDb::Internal::get_reducer_error();
        WriteBytes(error_sink, std::vector<uint8_t>(error_msg.begin(), error_msg.end()));
        return StatusCode::HOST_CALL_FAILURE;
    }    

    return StatusCode::OK;
}
}
}


 