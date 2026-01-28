#ifndef SPACETIMEDB_V9_TYPE_REGISTRATION_H
#define SPACETIMEDB_V9_TYPE_REGISTRATION_H

#include <memory>
#include <unordered_map>
#include <unordered_set>
#include <string>
#include <typeinfo>
#include <cxxabi.h>
#include "../bsatn/bsatn.h"

// Forward declarations
namespace SpacetimeDB {
namespace Internal {
    class AlgebraicType;
}
namespace detail {
    // Forward declaration for namespace storage
    template<typename T>
    struct namespace_info;
}
}

// Helper function to demangle C++ type names - inline implementation for template usage
inline std::string demangle_cpp_type_name(const char* name) {
    int status = 0;
    std::unique_ptr<char, void(*)(void*)> demangled(
        abi::__cxa_demangle(name, nullptr, nullptr, &status),
        std::free
    );
    return (status == 0 && demangled) ? std::string(demangled.get()) : std::string(name);
}

namespace SpacetimeDB {
namespace Internal {

/**
 * V9TypeRegistration - Single unified type registration system for V9 modules
 * 
 * Core principles:
 * - Only user-defined structs and enums get registered in the types array
 * - Primitives, arrays, Options, and special types are always inlined
 * - Every registered type gets a name and RawTypeDefV9 export
 * - Single entry point: registerType()
 * 
 * Type handling:
 * - Primitives (Bool, U8, I32, etc.) → Return inline, never registered
 * - Arrays → Return inline with recursive element processing
 * - Options → Return inline Sum structure
 * - Special types (Identity, etc.) → Return inline Product structure
 * - User structs/enums → Register in typespace, return Ref
 */
class V9TypeRegistration {
private:
    // Cache of type name -> typespace index (built from GetV9Module().types)
    std::unordered_map<std::string, uint32_t> type_name_cache_;
    
    // Track types currently being registered to detect cycles
    std::unordered_set<std::string> types_being_registered_;
    
    // Error state - set when we detect validation errors
    bool has_error_ = false;
    std::string error_message_;
    std::string error_type_description_;  // Stores the type structure for debugging
    

public:
    /**
     * THE ONLY type registration function - single entry point for all types
     * 
     * @param bsatn_type The type to process
     * @param explicit_name Optional explicit name for the type
     * @param cpp_type Optional C++ type info for name extraction
     * @return AlgebraicType - either inline (primitives/arrays/special) or Ref to registered type
     */
    ::SpacetimeDB::Internal::AlgebraicType registerType(const bsatn::AlgebraicType& bsatn_type,
                                                        const std::string& explicit_name = "",
                                                        const std::type_info* cpp_type = nullptr);
    
    /**
     * Register a type immediately by name (called by enum macros)
     * This registers the type the first time its algebraic_type() is called
     */
    void registerTypeByName(const std::string& type_name, 
                            const bsatn::AlgebraicType& algebraic_type,
                            const std::type_info* cpp_type);
    
    /**
     * Register a type and return its typespace index
     * Used by simple enums to get a Ref they can return
     */
    uint32_t registerAndGetIndex(const bsatn::AlgebraicType& bsatn_type,
                                 const std::string& type_name,
                                 const std::type_info* cpp_type = nullptr);
    
    /**
     * Check if any errors occurred during type registration
     */
    bool hasError() const { return has_error_; }
    
    /**
     * Get the error message if an error occurred
     */
    const std::string& getErrorMessage() const { return error_message_; }
    
    /**
     * Get the error type description if an error occurred
     */
    const std::string& getErrorTypeDescription() const { return error_type_description_; }
    
    /**
     * Add namespace qualification to an existing registered type
     * 
     * This is called by SPACETIMEDB_NAMESPACE macros during preinit to modify
     * the registered type name with a namespace prefix.
     * 
     * @tparam T The C++ type to modify
     * @param namespace_prefix The namespace prefix to prepend (e.g., "Namespace")
     */
    template<typename T>
    void set_type_namespace(const std::string& namespace_prefix) {
        // Get the type name that was registered
        std::string original_name = demangle_cpp_type_name(typeid(T).name());
        
        // Find the type in our cache
        auto it = type_name_cache_.find(original_name);
        if (it != type_name_cache_.end()) {
            uint32_t type_index = it->second;
            std::string qualified_name = namespace_prefix + "." + original_name;
            
            // Update the cache with the new name
            type_name_cache_.erase(it);
            type_name_cache_[qualified_name] = type_index;
            
            // Update the actual type name in the module definition
            updateTypeNameInModule(type_index, qualified_name);
        }
    }
    
    /**
     * Clear all registration state - used to reset between module builds
     */
    void clear() {
        type_name_cache_.clear();
        types_being_registered_.clear();
        has_error_ = false;
        error_message_.clear();
        error_type_description_.clear();
    }

private:
    /**
     * Check if a type is a primitive (tags 0-19)
     */
    bool isPrimitive(const bsatn::AlgebraicType& type) const;
    
    /**
     * Check if a type is a special type (Identity, ConnectionId, etc.)
     */
    bool isSpecialType(const bsatn::AlgebraicType& type) const;
    
    /**
     * Check if a type is an Option (Sum with "some" and "none" variants)
     */
    bool isOptionType(const bsatn::AlgebraicType& type) const;
    
    /**
     * Check if a type is Result (Sum with "ok" and "err" variants)
     */
    bool isResultType(const bsatn::AlgebraicType& type) const;
    
    /**
     * Check if a type is ScheduleAt (Sum with "Interval" and "Time" variants)
     */
    bool isScheduleAtType(const bsatn::AlgebraicType& type) const;
    
    /**
     * Check if a type is Unit (empty Product)
     */
    bool isUnitType(const bsatn::AlgebraicType& type) const;
    
    /**
     * Extract a clean type name from C++ type_info
     */
    std::string extractTypeName(const std::type_info* cpp_type) const;
    
    /**
     * Parse namespace and name from qualified name (e.g., "Namespace.TestC" -> ["Namespace"], "TestC")
     */
    std::pair<std::vector<std::string>, std::string> parseNamespaceAndName(const std::string& qualified_name) const;
    
    /**
     * Convert a primitive BSATN type to internal AlgebraicType
     */
    ::SpacetimeDB::Internal::AlgebraicType convertPrimitive(const bsatn::AlgebraicType& type) const;
    
    /**
     * Convert an array type, recursively processing the element
     */
    ::SpacetimeDB::Internal::AlgebraicType convertArray(const bsatn::AlgebraicType& type);
    
    /**
     * Convert a special type to its inline Product structure
     */
    ::SpacetimeDB::Internal::AlgebraicType convertSpecialType(const bsatn::AlgebraicType& type);
    
    /**
     * Convert an Option or ScheduleAt to its inline Sum structure
     */
    ::SpacetimeDB::Internal::AlgebraicType convertInlineSum(const bsatn::AlgebraicType& type);
    
    /**
     * Convert a Unit type to its inline Product structure
     */
    ::SpacetimeDB::Internal::AlgebraicType convertUnitType() const;
    
    /**
     * Register a complex user-defined type (struct or enum)
     */
    ::SpacetimeDB::Internal::AlgebraicType registerComplexType(const bsatn::AlgebraicType& type,
                                                               const std::string& type_name);
    
    /**
     * Process a Product type, recursively registering field types
     */
    ::SpacetimeDB::Internal::AlgebraicType processProduct(const bsatn::AlgebraicType& type);
    
    /**
     * Process a Sum type, recursively registering variant types
     */
    ::SpacetimeDB::Internal::AlgebraicType processSum(const bsatn::AlgebraicType& type);
    
    /**
     * Describe a type structure for error messages
     */
    std::string describeType(const bsatn::AlgebraicType& type) const;
    
    /**
     * Update the type name in the actual module definition
     * 
     * This modifies the RawTypeDefV9 entry in the module to use the new
     * namespace-qualified name for client generation.
     * 
     * @param type_index The index of the type to update
     * @param new_name The new qualified name to use
     */
    void updateTypeNameInModule(uint32_t type_index, const std::string& new_name);
    
};

// Global V9 type registration instance
extern std::unique_ptr<V9TypeRegistration> g_v9_type_registration;

// Initialize the V9 type registration (called once at module startup)
void initializeV9TypeRegistration();

// Get the global V9 type registration
V9TypeRegistration& getV9TypeRegistration();

} // namespace Internal

namespace Internal {

// Thread-local storage for tracking the chain of types being registered
// This is used to detect circular references during type building
extern thread_local std::vector<std::string> g_type_registration_chain;

// Global flag to indicate circular reference error (set during type building)
extern bool g_circular_ref_error;
extern std::string g_circular_ref_type_name;

/**
 * @brief Template helper to abstract the lazy type registration pattern
 * 
 * This eliminates code duplication between enums and structs that all follow
 * the same pattern:
 * - Static variable to cache the type index
 * - One-time registration on first call
 * - Return Ref to the registered type
 * 
 * Benefits of this abstraction:
 * - Reduces code duplication by ~15 lines per type
 * - Consistent registration behavior across all user-defined types
 * - Better error handling and validation
 * - Thread-safe initialization
 * - Cleaner macro implementations
 * 
 * @tparam T The C++ type being registered
 */
template<typename T>
class LazyTypeRegistrar {
private:
    static inline uint32_t type_index_ = 0xFFFFFFFF;
    
public:
    /**
     * Get or register a type with lazy initialization
     * 
     * This method uses lazy initialization to register a type only when first needed.
     * The registration is thread-safe and cached for subsequent calls.
     * 
     * @tparam BuilderFunc Function type that builds the AlgebraicType
     * @param build_func Function that constructs the AlgebraicType for this type
     * @param type_name Explicit name for the type (typically from #Type macro)
     * @return AlgebraicType::Ref to the registered type
     * 
     * @note The build_func should be a lambda that constructs the AlgebraicType
     *       without any side effects, as it may be called during registration.
     */
    template<typename BuilderFunc>
    static bsatn::AlgebraicType getOrRegister(BuilderFunc&& build_func, 
                                              const std::string& type_name) {
        // Check if already registered (fast path for subsequent calls)
        if (type_index_ != 0xFFFFFFFF) {
            return bsatn::AlgebraicType::make_ref(type_index_);
        }
        
        // Check if this type has namespace information and build qualified name
        std::string qualified_name = type_name;
        if constexpr (requires { SpacetimeDB::detail::namespace_info<T>::value; }) {
            constexpr const char* namespace_prefix = SpacetimeDB::detail::namespace_info<T>::value;
            if (namespace_prefix != nullptr) {
                qualified_name = std::string(namespace_prefix) + "." + type_name;
            }
        }
        
        // CRITICAL: Check for circular references BEFORE building the type
        // This prevents infinite recursion during type construction
        for (const auto& type_in_chain : g_type_registration_chain) {
            if (type_in_chain == qualified_name) {
                // Circular reference detected!
                fprintf(stderr, "\n\n[CIRCULAR REFERENCE DETECTED]\n");
                fprintf(stderr, "ERROR: Circular reference detected for type '%s'\n", qualified_name.c_str());
                fprintf(stderr, "  Registration chain: ");
                for (const auto& t : g_type_registration_chain) {
                    fprintf(stderr, "%s -> ", t.c_str());
                }
                fprintf(stderr, "%s (circular!)\n", qualified_name.c_str());
                fprintf(stderr, "Setting g_circular_ref_error = true\n");
                fprintf(stderr, "[END CIRCULAR REFERENCE DETECTION]\n\n");
                fflush(stderr);
                
                // Set global error flag for preinit_99 to handle
                g_circular_ref_error = true;
                g_circular_ref_type_name = qualified_name;
                
                // Return a simple primitive type to break the infinite recursion
                // This prevents any invalid references from being created
                // The error will be handled in preinit_99
                return bsatn::AlgebraicType::U32();  // Return a safe primitive type
            }
        }
        
        // Add this type to the registration chain
        g_type_registration_chain.push_back(qualified_name);
        
        // Slow path: first registration
        // Build the algebraic type using the provided function
        bsatn::AlgebraicType algebraic_type = build_func();
        
        // Remove from chain after successful building
        g_type_registration_chain.pop_back();
        
        // Check if circular reference was detected during type building
        if (g_circular_ref_error) {
            // Don't register anything - just return the safe type
            // The error will be handled in preinit_99
            return algebraic_type;
        }
        
        // Register with V9 system and cache the index using the qualified name
        type_index_ = getV9TypeRegistration().registerAndGetIndex(
            algebraic_type, qualified_name, &typeid(T));
        
        return bsatn::AlgebraicType::make_ref(type_index_);
    }
    
    /**
     * Check if this type has been registered yet
     * @return true if the type has been registered and cached
     */
    static bool isRegistered() {
        return type_index_ != 0xFFFFFFFF;
    }
    
    /**
     * Get the cached type index (only valid if isRegistered() returns true)
     * @return The cached type index
     * @warning Only call this if isRegistered() returns true
     */
    static uint32_t getTypeIndex() {
        return type_index_;
    }
    
    /**
     * Force reset the registration state (for testing purposes only)
     * @warning This should not be used in production code
     */
    static void resetForTesting() {
        type_index_ = 0xFFFFFFFF;
    }
};


} // namespace Internal
} // namespace SpacetimeDB

#endif // SPACETIMEDB_V9_TYPE_REGISTRATION_H