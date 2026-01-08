#ifndef SPACETIMEDB_NESTED_TYPE_COLLECTION_H
#define SPACETIMEDB_NESTED_TYPE_COLLECTION_H

// TypeRegistry system removed - use V9TypeRegistration instead
// This entire file is deprecated as it was designed for the legacy TypeRegistry system
/*
// type_name_registry.h removed - functionality moved to TypeRegistry
#include "../bsatn/traits.h"
#include <vector>
#include <optional>
#include <variant>

namespace SpacetimeDb {

namespace Internal {

// Forward declarations - these are defined in Module_impl.h
template<typename T> struct is_optional;
template<typename T> struct is_vector_type;

// Enhanced version of collect_single_param_type that properly handles nested types
template<typename T>
void collect_single_param_type_v2(TypeRegistry& registry) {
    // CRITICAL DEDUPLICATION: Check if this type is already registered FIRST
    // This prevents the massive over-registration that was causing 2.4x type inflation
    if (registry.find_type_index_by_cpp_type(&typeid(T)) != static_cast<uint32_t>(-1)) {
        return; // Type already registered - skip collection entirely
    }
    
    // Early exit for primitive types (including big integers)
    constexpr bool is_primitive = std::is_same_v<T, bool> || 
                                 std::is_same_v<T, uint8_t> || std::is_same_v<T, uint16_t> ||
                                 std::is_same_v<T, uint32_t> || std::is_same_v<T, uint64_t> ||
                                 std::is_same_v<T, int8_t> || std::is_same_v<T, int16_t> ||
                                 std::is_same_v<T, int32_t> || std::is_same_v<T, int64_t> ||
                                 std::is_same_v<T, float> || std::is_same_v<T, double> ||
                                 std::is_same_v<T, std::string> ||
                                 std::is_same_v<T, ::SpacetimeDb::u128> || std::is_same_v<T, ::SpacetimeDb::i128> ||
                                 std::is_same_v<T, ::SpacetimeDb::u256> || std::is_same_v<T, ::SpacetimeDb::i256>;
    
    if constexpr (is_primitive) {
        return; // Primitive types don't need registration
    }
    
    // Handle special SpacetimeDB types separately
    if constexpr (std::is_same_v<T, ::SpacetimeDb::Identity> ||
                  std::is_same_v<T, ::SpacetimeDb::ConnectionId> ||
                  std::is_same_v<T, ::SpacetimeDb::Timestamp> ||
                  std::is_same_v<T, ::SpacetimeDb::TimeDuration> ||
                  std::is_same_v<T, ::SpacetimeDb::ScheduleAt>) {
        // Special types should NOT be registered - they should be inlined
        // Rust inlines these types completely  
        return;
    }
    
    if constexpr (is_vector_type<T>::value) {
        using element_type = typename T::value_type;
        
        // First, recursively collect the element type (unless it's a special type)
        if constexpr (!(std::is_same_v<element_type, ::SpacetimeDb::Identity> ||
                       std::is_same_v<element_type, ::SpacetimeDb::ConnectionId> ||
                       std::is_same_v<element_type, ::SpacetimeDb::Timestamp> ||
                       std::is_same_v<element_type, ::SpacetimeDb::TimeDuration>)) {
            collect_single_param_type_v2<element_type>(registry);
        }
        
        // Now create the vector type with proper element reference
        uint32_t elem_ref = static_cast<uint32_t>(-1);  // Initialize to invalid
        
        // Determine element type reference
        if constexpr (std::is_same_v<element_type, bool>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::Bool);
        } else if constexpr (std::is_same_v<element_type, uint8_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U8);
        } else if constexpr (std::is_same_v<element_type, uint16_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U16);
        } else if constexpr (std::is_same_v<element_type, uint32_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U32);
        } else if constexpr (std::is_same_v<element_type, uint64_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U64);
        } else if constexpr (std::is_same_v<element_type, int8_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I8);
        } else if constexpr (std::is_same_v<element_type, int16_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I16);
        } else if constexpr (std::is_same_v<element_type, int32_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I32);
        } else if constexpr (std::is_same_v<element_type, int64_t>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I64);
        } else if constexpr (std::is_same_v<element_type, float>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::F32);
        } else if constexpr (std::is_same_v<element_type, double>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::F64);
        } else if constexpr (std::is_same_v<element_type, std::string>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::String);
        } else if constexpr (std::is_same_v<element_type, ::SpacetimeDb::u128>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U128);
        } else if constexpr (std::is_same_v<element_type, ::SpacetimeDb::i128>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I128);
        } else if constexpr (std::is_same_v<element_type, ::SpacetimeDb::u256>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U256);
        } else if constexpr (std::is_same_v<element_type, ::SpacetimeDb::i256>) {
            elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I256);
        } else {
            // Complex type - need to find it in the registry
            // For Option<T>, we need to register it first
            if constexpr (is_optional<element_type>::value) {
                // First register the Option<T> type by calling its collection function
                collect_single_param_type_v2<element_type>(registry);
                
                // Set the thread-local registry for algebraic_type generation
                
                // Now get the Option<T> type and find it in the registry
                auto option_type = bsatn::bsatn_traits<element_type>::algebraic_type();
                
                // Restore old registry
                
                // Options are never registered - they are inlined when needed
                elem_ref = static_cast<uint32_t>(-1); // Mark as not found, will be inlined
            } else if constexpr (std::is_same_v<element_type, ::SpacetimeDb::Identity> ||
                                 std::is_same_v<element_type, ::SpacetimeDb::ConnectionId> ||
                                 std::is_same_v<element_type, ::SpacetimeDb::Timestamp> ||
                                 std::is_same_v<element_type, ::SpacetimeDb::TimeDuration>) {
                // CRITICAL: Special types should NEVER be registered - they must be inlined
                // For Array<SpecialType>, we'll create the array with the special type inlined
                // Don't set elem_ref - handle this case specially below
            } else if constexpr (std::is_enum_v<element_type>) {
                // Enum type - register it if it has custom bsatn_traits (like Sum types)
                if constexpr (requires { bsatn::bsatn_traits<element_type>::algebraic_type(); }) {
                    // First collect the enum type itself
                    collect_single_param_type_v2<element_type>(registry);
                    
                    // Then find/register the enum type
                    auto enum_type = bsatn::bsatn_traits<element_type>::algebraic_type();
                    
                    elem_ref = registry.find_type_index(enum_type);
                    if (elem_ref == static_cast<uint32_t>(-1)) {
                        // Not found - register it WITH type info to preserve name
                        elem_ref = registry.register_type_with_info(enum_type, &typeid(element_type));
                    }
                } else {
                    // Plain enum without custom traits - use underlying type
                    using underlying = std::underlying_type_t<element_type>;
                    if constexpr (std::is_same_v<underlying, uint8_t>) {
                        elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U8);
                    } else if constexpr (std::is_same_v<underlying, uint16_t>) {
                        elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U16);
                    } else if constexpr (std::is_same_v<underlying, uint32_t>) {
                        elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U32);
                    } else {
                        // Other underlying types can be added as needed
                        elem_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U8);
                    }
                }
            } else if constexpr (requires { bsatn::bsatn_traits<element_type>::algebraic_type(); }) {
                // Regular struct type
                // CRITICAL FIX: Must collect the element type FIRST before trying to find it!
                // This was causing BTreeU32 fallback because types weren't registered yet
                collect_single_param_type_v2<element_type>(registry);
                
                // Set the thread-local registry for type generation
                
                auto elem_type = bsatn::bsatn_traits<element_type>::algebraic_type();
                
                // Restore old registry
                elem_ref = registry.find_type_index(elem_type);
                if (elem_ref == static_cast<uint32_t>(-1)) {
                    // Not found - register it with type info to preserve name
                    elem_ref = registry.register_type_with_info(elem_type, &typeid(element_type));
                }
            } else {
                // Fallback for any other complex types
                // This should not happen if all types are properly handled above
                static_assert(sizeof(element_type) == 0, "Unsupported element type in vector");
            }
        }
        
        // Ensure we have a valid elem_ref before creating the array type
        // Note: We can't throw exceptions in WASM modules during initialization
        
        // ARRAYS ARE NEVER REGISTERED IN TYPESPACE - they are always inlined like in Rust
        // The bsatn_traits<std::vector<T>>::algebraic_type() will handle array creation
        // when the type is actually needed for serialization or client generation
        
    } else if constexpr (is_optional<T>::value) {
        using inner_type = typename T::value_type;
        
        // Process all optional types, including complex nested ones
        // Now that we properly handle TypeRegistry context, we can support
        // optional<vector<T>> and other complex nested types
        
        // First, recursively collect the inner type (unless it's a special type)
        if constexpr (!(std::is_same_v<inner_type, ::SpacetimeDb::Identity> ||
                       std::is_same_v<inner_type, ::SpacetimeDb::ConnectionId> ||
                       std::is_same_v<inner_type, ::SpacetimeDb::Timestamp> ||
                       std::is_same_v<inner_type, ::SpacetimeDb::TimeDuration>)) {
            collect_single_param_type_v2<inner_type>(registry);
        }
        
        // Determine inner type reference for Some variant
        uint32_t inner_ref = static_cast<uint32_t>(-1);  // Initialize to invalid
        
        if constexpr (std::is_same_v<inner_type, bool>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::Bool);
        } else if constexpr (std::is_same_v<inner_type, uint8_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U8);
        } else if constexpr (std::is_same_v<inner_type, uint16_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U16);
        } else if constexpr (std::is_same_v<inner_type, uint32_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U32);
        } else if constexpr (std::is_same_v<inner_type, uint64_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U64);
        } else if constexpr (std::is_same_v<inner_type, int8_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I8);
        } else if constexpr (std::is_same_v<inner_type, int16_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I16);
        } else if constexpr (std::is_same_v<inner_type, int32_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I32);
        } else if constexpr (std::is_same_v<inner_type, int64_t>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I64);
        } else if constexpr (std::is_same_v<inner_type, float>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::F32);
        } else if constexpr (std::is_same_v<inner_type, double>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::F64);
        } else if constexpr (std::is_same_v<inner_type, std::string>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::String);
        } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::u128>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U128);
        } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::i128>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I128);
        } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::u256>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U256);
        } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::i256>) {
            inner_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I256);
        } else {
            // Complex type - handle vectors specially
            if constexpr (is_vector_type<inner_type>::value) {
                // For vectors inside optionals, we need to handle them carefully
                // The vector needs its element type to be registered first
                using vector_element = typename inner_type::value_type;
                
                // First ensure the vector's element type is collected
                collect_single_param_type_v2<vector_element>(registry);
                
                // Now register the vector type itself
                // Set the thread-local registry for type generation
                
                auto inner_alg_type = bsatn::bsatn_traits<inner_type>::algebraic_type();
                
                // Restore old registry
                
                inner_ref = registry.find_type_index(inner_alg_type);
                if (inner_ref == static_cast<uint32_t>(-1)) {
                    inner_ref = registry.register_type(inner_alg_type);
                }
            } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::Identity> ||
                          std::is_same_v<inner_type, ::SpacetimeDb::ConnectionId> ||
                          std::is_same_v<inner_type, ::SpacetimeDb::Timestamp> ||
                          std::is_same_v<inner_type, ::SpacetimeDb::TimeDuration>) {
                // Special SpacetimeDB types should NEVER be registered
                // For Option<SpecialType>, we'll inline the special type directly
                // Don't set inner_ref - handle this case specially below
            } else if constexpr (requires { bsatn::bsatn_traits<inner_type>::algebraic_type(); }) {
                // CRITICAL FIX: Must collect the inner type FIRST before trying to find it!
                // This was causing BTreeU32 fallback for Option<StructType>
                collect_single_param_type_v2<inner_type>(registry);
                
                // Set the thread-local registry for type generation
                
                auto inner_alg_type = bsatn::bsatn_traits<inner_type>::algebraic_type();
                
                // Restore old registry
                inner_ref = registry.find_type_index(inner_alg_type);
                if (inner_ref == static_cast<uint32_t>(-1)) {
                    inner_ref = registry.register_type_with_info(inner_alg_type, &typeid(inner_type));
                }
            } else {
                // This should not happen
                static_assert(sizeof(inner_type) == 0, "Unsupported inner type in optional");
            }
        }
        
        // Ensure we have a valid inner_ref before creating the option type
        
        // Create Option type with proper inline handling
        std::vector<bsatn::SumTypeVariant> variants;
        
        // Check if inner_type is a special type that needs inlining
        if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::Identity> ||
                      std::is_same_v<inner_type, ::SpacetimeDb::ConnectionId> ||
                      std::is_same_v<inner_type, ::SpacetimeDb::Timestamp> ||
                      std::is_same_v<inner_type, ::SpacetimeDb::TimeDuration>) {
            // Create the special type inline for the "some" variant
            std::vector<bsatn::ProductTypeElement> elements;
            if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::Identity>) {
                elements.emplace_back(bsatn::IDENTITY_TAG, bsatn::AlgebraicType::U256());
            } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::ConnectionId>) {
                elements.emplace_back(bsatn::CONNECTION_ID_TAG, bsatn::AlgebraicType::U128());
            } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::Timestamp>) {
                elements.emplace_back(bsatn::TIMESTAMP_TAG, bsatn::AlgebraicType::I64());
            } else if constexpr (std::is_same_v<inner_type, ::SpacetimeDb::TimeDuration>) {
                elements.emplace_back(bsatn::TIME_DURATION_TAG, bsatn::AlgebraicType::I64());
            }
            
            auto special_product = std::make_unique<bsatn::ProductType>(std::move(elements));
            auto special_type = bsatn::AlgebraicType::make_product(std::move(special_product));
            
            // Use the inline special type for the "some" variant
            variants.emplace_back("some", std::move(special_type));
        } else {
            // Regular type - but check if it's a sentinel value
            if (inner_ref == 0xFFFFFFFF) {
                // This inner type is a special type that should be inlined
                // Get the actual type and inline it
                auto inner_alg_type = bsatn::bsatn_traits<inner_type>::algebraic_type();
                variants.emplace_back("some", std::move(inner_alg_type));
            } else {
                // Valid reference - use it
                variants.emplace_back("some", bsatn::AlgebraicType::Ref(inner_ref));
            }
        }
        
        // CRITICAL FIX: The "none" variant should be an INLINE empty Product, NOT a reference!
        // This matches Rust behavior where Option's None variant contains Unit directly
        // This prevents registering empty Products in the typespace for every Option type
        auto none_unit = bsatn::AlgebraicType::make_product(
            std::make_unique<bsatn::ProductType>(std::vector<bsatn::ProductTypeElement>{})
        );
        variants.emplace_back("none", std::move(none_unit));
        
        // OPTIONS ARE NEVER REGISTERED IN TYPESPACE - they are always inlined like in Rust
        // The bsatn_traits<std::optional<T>>::algebraic_type() will handle option creation
        // when the type is actually needed for serialization or client generation
        
    } else {
        // Handle non-primitive types (structs, variants, enums with custom traits)
        
        // CRITICAL: Check for SPACETIMEDB_VARIANT_ENUM wrapper structs FIRST
        // These have a variant_type member and special bsatn_traits that return Sum types
        if constexpr (requires { typename T::variant_type; }) {
            
            // Get the type name for proper registration
            std::string type_name = demangle_cpp_type_name(typeid(T).name());
            
            // Clean up the demangled name
            size_t pos = type_name.rfind("::");
            if (pos != std::string::npos) {
                type_name = type_name.substr(pos + 2);
            } else if (!type_name.empty() && std::isdigit(type_name[0])) {
                size_t i = 0;
                while (i < type_name.length() && std::isdigit(type_name[i])) i++;
                type_name = type_name.substr(i);
            }
            
            
            // Set thread-local registry for type generation
            
            // Get the Sum type from the wrapper's bsatn_traits
            auto variant_type = bsatn::bsatn_traits<T>::algebraic_type();
            
            // Restore old registry
            
            // Register with type info to preserve name
            uint32_t type_ref = registry.register_type_with_info(variant_type, &typeid(T));
            
            // Ensure name is registered
            if (!type_name.empty()) {
                registry.register_type_name(type_ref, type_name);
            }
            
            return; // Done with variant wrapper
        }
        
        // For enums with custom bsatn_traits, register with type info
        if constexpr (std::is_enum_v<T> && requires { bsatn::bsatn_traits<T>::algebraic_type(); }) {
            // UNIFIED: Use same name extraction as structs/variants
            std::string type_name = demangle_cpp_type_name(typeid(T).name());
            
            // Clean up the demangled name
            size_t pos = type_name.rfind("::");
            if (pos != std::string::npos) {
                type_name = type_name.substr(pos + 2);
            } else if (!type_name.empty() && std::isdigit(type_name[0])) {
                size_t i = 0;
                while (i < type_name.length() && std::isdigit(type_name[i])) i++;
                type_name = type_name.substr(i);
            }
            
            
            // Set the global registry temporarily for enum registration
            
            auto enum_type = bsatn::bsatn_traits<T>::algebraic_type();
            // UNIFIED API: Register enum with both type identity and name in one call
            [[maybe_unused]] uint32_t type_ref = registry.register_type(enum_type, {
                .cpp_type = &typeid(T),
                .type_name = type_name
            });
            
            // Restore old registry
        } else if constexpr (requires { typename T::_Base; } || requires { typename T::variant_type; }) { // std::variant has _Base, wrapper has variant_type
            // This is a std::variant or variant wrapper - need to handle it specially
            
            // CRITICAL: For wrapper structs, we want to register the Sum type, not a Product
            // The SPACETIMEDB_VARIANT_ENUM macro creates a wrapper struct with a specialized
            // bsatn_traits that returns a Sum type
            if constexpr (requires { typename T::variant_type; }) {
            }
            
            // First, recursively collect all variant types
            if constexpr (requires { bsatn::bsatn_traits<T>::algebraic_type(); }) {
                // CRITICAL FIX: Get variant name directly from demangling, eliminating TypeNameRegistry
                std::string type_name = demangle_cpp_type_name(typeid(T).name());
                
                // Clean up the demangled name to extract just the type name
                size_t pos = type_name.rfind("::");
                if (pos != std::string::npos) {
                    type_name = type_name.substr(pos + 2);
                } else if (!type_name.empty() && std::isdigit(type_name[0])) {
                    // Handle simple mangled names
                    size_t i = 0;
                    while (i < type_name.length() && std::isdigit(type_name[i])) i++;
                    type_name = type_name.substr(i);
                }
                
                
                // Set the global registry temporarily for variant registration
                
                auto variant_type = bsatn::bsatn_traits<T>::algebraic_type();
                
                // Use register_type_with_info to ensure proper name registration
                uint32_t type_ref = registry.register_type_with_info(variant_type, &typeid(T));
                
                // CRITICAL: Ensure the name is registered in the local TypeRegistry
                // register_type_with_info might not have the correct name, so register it explicitly
                if (!type_name.empty()) {
                    registry.register_type_name(type_ref, type_name);
                }
                
                // Restore old registry
            }
        } else {
            // Regular struct type
            if constexpr (requires { bsatn::bsatn_traits<T>::algebraic_type(); }) {
                // CRITICAL FIX: Get type name directly from demangling, eliminating TypeNameRegistry dependency
                // This unifies the system to use a single registry with consistent naming
                std::string type_name = demangle_cpp_type_name(typeid(T).name());
                
                // Clean up the demangled name to extract just the type name
                size_t pos = type_name.rfind("::");
                if (pos != std::string::npos) {
                    type_name = type_name.substr(pos + 2);
                } else if (!type_name.empty() && std::isdigit(type_name[0])) {
                    // Handle simple mangled names like "13MinimalStruct" -> "MinimalStruct"
                    size_t i = 0;
                    while (i < type_name.length() && std::isdigit(type_name[i])) i++;
                    type_name = type_name.substr(i);
                }
                
                
                // CRITICAL FIX: Before generating the algebraic type, we need to
                // recursively collect all nested field types. This ensures that
                // when ProductTypeBuilder tries to reference field types, they're
                // already registered.
                // 
                // For wrapper structs like VecU8 { n: std::vector<uint8_t> },
                // we need to ensure the vector type is registered first.
                //
                // Since we can't easily introspect struct fields at compile time,
                // we'll generate the algebraic type twice - once to discover field types,
                // then again after registering them.
                
                // CRITICAL FIX: Set thread-local registry for type generation
                
                auto param_type = bsatn::bsatn_traits<T>::algebraic_type();
                
                // Restore old registry
                
                // Register the type with type info to enable TypeNameRegistry lookup
                // This allows Module.cpp to find the proper struct name
                // Type registration complete
                
                uint32_t registered_ref = registry.register_type_with_info(param_type, &typeid(T));
                
                // CRITICAL: Ensure the name is registered in the local TypeRegistry
                // register_type_with_info might not have the correct name, so register it explicitly
                if (!type_name.empty()) {
                    registry.register_type_name(registered_ref, type_name);
                        
                    // Name registration completed
                }
                
                // Type registration completed
                
            }
        }
    }
}

// UNIFIED TYPE REFERENCE FUNCTION
// Gets the appropriate type reference for any type T:
// - Primitives: returns AlgebraicTypeTag
// - Special types: returns special marker
// - Complex types: ensures registered and returns index
template<typename T>
uint32_t get_type_reference(TypeRegistry& registry) {
    // No need for context switching with global registry
    uint32_t type_ref = 0;
    
    // Primitive types - return their AlgebraicTypeTag directly
    if constexpr (std::is_same_v<T, bool>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::Bool);
    } else if constexpr (std::is_same_v<T, uint8_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U8);
    } else if constexpr (std::is_same_v<T, uint16_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U16);
    } else if constexpr (std::is_same_v<T, uint32_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U32);
    } else if constexpr (std::is_same_v<T, uint64_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U64);
    } else if constexpr (std::is_same_v<T, int8_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I8);
    } else if constexpr (std::is_same_v<T, int16_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I16);
    } else if constexpr (std::is_same_v<T, int32_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I32);
    } else if constexpr (std::is_same_v<T, int64_t>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I64);
    } else if constexpr (std::is_same_v<T, float>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::F32);
    } else if constexpr (std::is_same_v<T, double>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::F64);
    } else if constexpr (std::is_same_v<T, std::string>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::String);
    } else if constexpr (std::is_same_v<T, ::SpacetimeDb::u128>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U128);
    } else if constexpr (std::is_same_v<T, ::SpacetimeDb::i128>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I128);
    } else if constexpr (std::is_same_v<T, ::SpacetimeDb::u256>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::U256);
    } else if constexpr (std::is_same_v<T, ::SpacetimeDb::i256>) {
        type_ref = static_cast<uint32_t>(bsatn::AlgebraicTypeTag::I256);
    }
    // Special types - NEVER register them, they must be inlined
    // Return INVALID_TYPE_ID to signal that this type should be inlined wherever used
    else if constexpr (std::is_same_v<T, ::SpacetimeDb::Identity> ||
                      std::is_same_v<T, ::SpacetimeDb::ConnectionId> ||
                      std::is_same_v<T, ::SpacetimeDb::Timestamp> ||
                      std::is_same_v<T, ::SpacetimeDb::TimeDuration> ||
                      std::is_same_v<T, ::SpacetimeDb::ScheduleAt>) {
        // Special types should NOT be registered in the typespace
        // They are always inlined at their point of use
        type_ref = TypeConstants::INVALID_TYPE_ID;
    }
    // Complex types - ensure registered and return index
    else {
        // CRITICAL: Only collect/register types during module initialization
        // After init, all types should already be in the registry
        // Ensure the type is collected/registered
        collect_single_param_type_v2<T>(registry);
        
        // For EnumWithPayload and other complex types that have names,
        // look them up by type info rather than calling algebraic_type() again
        if constexpr (requires { typename T::variant_type; } || std::is_enum_v<T>) {
            // This is likely a named type - find it by type info
            type_ref = registry.find_type_index_by_cpp_type(&typeid(T));
            
            if (type_ref != static_cast<uint32_t>(-1)) {
                fprintf(stdout, "DEBUG: Found complex type %s at index=%u via type_info\n",
                        typeid(T).name(), type_ref);
            } else {
                // Fallback: try to find by algebraic type structure
                auto alg_type = bsatn::bsatn_traits<T>::algebraic_type();
                type_ref = registry.find_type_index(alg_type);
                // Found type via algebraic type structure
            }
        } else {
            // Other complex types - use the original approach  
            auto alg_type = bsatn::bsatn_traits<T>::algebraic_type();
            
            // CRITICAL: Handle Array types specially to prevent BTreeU32 substitution  
            // Arrays need special handling when registered in typespace
            if (alg_type.tag() == bsatn::AlgebraicTypeTag::Array) {
                // Array type detected
                
                // Arrays should NOT be registered in typespace - they should be inlined
                // However, if we don't register them, we get BTreeU32 substitution
                // So we need to register them but ensure element types are correct
                
                // Just register the Array as-is and let the codegen handle it
                type_ref = registry.register_type(alg_type);
                // Array registered
            } else {
                type_ref = registry.find_type_index(alg_type);
                // Found complex type
            }
        }
        
        if (type_ref == static_cast<uint32_t>(-1)) {
            // Type not found - register it now
            // Type not found - register it now
            auto alg_type = bsatn::bsatn_traits<T>::algebraic_type();
            type_ref = registry.register_type_with_info(alg_type, &typeid(T));
        }
    }
    
    return type_ref;
}

// Runtime version of get_type_reference for AlgebraicType objects
// This provides the same unified registration/lookup logic for runtime types
inline uint32_t get_type_reference_runtime(TypeRegistry& registry, const bsatn::AlgebraicType& type) {
    // Check for primitive types by tag
    // CRITICAL FIX: Primitive types are String (4) through F64 (19), NOT everything <= F64!
    // Product (2) was incorrectly being treated as primitive due to bad comparison
    if ((type.tag() >= bsatn::AlgebraicTypeTag::String && type.tag() <= bsatn::AlgebraicTypeTag::F64)) {
        // Primitive types use their tag directly
        return static_cast<uint32_t>(type.tag());
    }
    
    // CRITICAL: Handle Array types specially to prevent BTreeU32 substitution
    // Register Arrays normally instead of using markers - let serialization handle them correctly
    if (type.tag() == bsatn::AlgebraicTypeTag::Array) {
        fprintf(stdout, "DEBUG: Runtime Array type detected - registering in typespace\n");
        
        // Register the Array type normally like other complex types
        uint32_t array_ref = registry.register_type(type);
        fprintf(stdout, "DEBUG: Runtime Array registered at type index: %u\n", array_ref);
        return array_ref;
    }
    
    // CRITICAL: Handle special types properly - they should NEVER be registered
    // Special types need to be handled inline during serialization, not as type references
    bsatn::SpecialTypeKind special = bsatn::get_special_type_kind(type);
    if (special != bsatn::SpecialTypeKind::None && special != bsatn::SpecialTypeKind::Option) {
        fprintf(stdout, "DEBUG: Runtime special type detected: %d - returning INVALID_TYPE_ID for inline serialization\n", static_cast<int>(special));
        // Special types must be inlined, never registered
        return TypeConstants::INVALID_TYPE_ID;
    }
    
    // CRITICAL: Check for special array markers - these should NOT be registered
    if (type.tag() == bsatn::AlgebraicTypeTag::Ref) {
        uint32_t ref_value = type.as_ref();
        // Check if this is a special array marker (0x60000000 range)
        if (ref_value >= 0x60000000 && ref_value < 0x70000000) {
            // Return the marker directly without registering
            return ref_value;
        }
    }
    
    // For complex types, try to find existing registration
    uint32_t existing_ref = registry.find_type_index(type);
    if (existing_ref != static_cast<uint32_t>(-1)) {
        return existing_ref;
    }
    
    // Not found - but check one more time if this is a special type that slipped through
    // This is a safety net to prevent special types from being registered
    if (type.tag() == bsatn::AlgebraicTypeTag::Product) {
        const auto& product = type.as_product();
        if (product.elements.size() == 1 && product.elements[0].name.has_value()) {
            const std::string& field_name = *product.elements[0].name;
            // Check for special type field names
            if (field_name == bsatn::IDENTITY_TAG ||
                field_name == bsatn::CONNECTION_ID_TAG ||
                field_name == bsatn::TIMESTAMP_TAG ||
                field_name == bsatn::TIME_DURATION_TAG ||
                field_name == "__identity__" ||
                field_name == "__connection_id__" ||
                field_name == "__timestamp_micros_since_unix_epoch__" ||
                field_name == "__time_duration_micros__") {
                fprintf(stderr, "WARNING: Special type '%s' almost registered! Returning INVALID_TYPE_ID\n", field_name.c_str());
                return TypeConstants::INVALID_TYPE_ID;
            }
        }
    }
    
    // Not found - register it
    uint32_t new_ref = registry.register_type(type);
    return new_ref;
}

} // namespace Internal
} // namespace SpacetimeDb
*/

#endif // SPACETIMEDB_NESTED_TYPE_COLLECTION_H