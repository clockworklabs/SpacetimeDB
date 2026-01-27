#ifndef SPACETIMEDB_FIELD_REGISTRATION_H
#define SPACETIMEDB_FIELD_REGISTRATION_H

#include <cstdint>
#include <cstddef>
#include <string>
#include <vector>
#include <optional>
#include <variant>
#include <type_traits>
#include "../macros.h"
#include <functional>
#include <map>
#include <typeinfo>
#include "spacetimedb/bsatn/types.h"
#include "spacetimedb/bsatn/timestamp.h"
#include "spacetimedb/bsatn/algebraic_type.h"
#include "spacetimedb/bsatn/writer.h"
#include "spacetimedb/bsatn/traits.h"

namespace SpacetimeDB {

// Helper templates for type detection
template<typename T>
struct is_vector : std::false_type {};
template<typename T>
struct is_vector<std::vector<T>> : std::true_type {};

template<typename T>
struct is_optional : std::false_type {};
template<typename T>
struct is_optional<std::optional<T>> : std::true_type {};

// =============================================================================
// Field Registration System
// =============================================================================
// 
// This system provides runtime metadata about struct fields for SpacetimeDB
// table registration. It bridges C++ compile-time type information to 
// SpacetimeDB's runtime type system.
//
// Primary use cases:
// - Table schema generation from C++ structs
// - Field constraint application
// - Type validation
// - Cross-language compatibility
//
// Note: For most use cases, prefer SPACETIMEDB_STRUCT from traits.h
// which provides both serialization and field registration in one macro.
// =============================================================================

// -----------------------------------------------------------------------------
// Type System Mapping
// -----------------------------------------------------------------------------

// Type traits for BSATN type mapping - simplified to use AlgebraicTypeTag directly
template<typename T> 
struct bsatn_type_id { 
    static constexpr bool is_primitive = false;
    static constexpr uint8_t value = 0; 
};

// Primitive type specializations
template<> struct bsatn_type_id<bool> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::Bool); 
};
template<> struct bsatn_type_id<uint8_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U8); 
};
template<> struct bsatn_type_id<uint16_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U16); 
};
template<> struct bsatn_type_id<uint32_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U32); 
};
template<> struct bsatn_type_id<uint64_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U64); 
};
template<> struct bsatn_type_id<int8_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::I8); 
};
template<> struct bsatn_type_id<int16_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::I16); 
};
template<> struct bsatn_type_id<int32_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::I32); 
};
template<> struct bsatn_type_id<int64_t> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::I64); 
};
template<> struct bsatn_type_id<float> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::F32); 
};
template<> struct bsatn_type_id<double> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::F64); 
};
template<> struct bsatn_type_id<std::string> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::String); 
};

// SpacetimeDB special types
template<> struct bsatn_type_id<Identity> { 
    static constexpr bool is_primitive = false;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::Product); 
};
template<> struct bsatn_type_id<ConnectionId> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U64); 
};
template<> struct bsatn_type_id<Timestamp> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U64); 
};
template<> struct bsatn_type_id<SpacetimeDB::u128> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U128); 
};
template<> struct bsatn_type_id<SpacetimeDB::i128> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::I128); 
};
template<> struct bsatn_type_id<SpacetimeDB::u256> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::U256); 
};
template<> struct bsatn_type_id<SpacetimeDB::i256> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::I256); 
};
template<> struct bsatn_type_id<ScheduleAt> { 
    static constexpr bool is_primitive = false;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::Sum); 
};

// Container types
template<typename T> 
struct bsatn_type_id<std::vector<T>> { 
    static constexpr bool is_primitive = false;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::Array); 
};

template<typename T> 
struct bsatn_type_id<std::optional<T>> { 
    static constexpr bool is_primitive = false;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::Sum); 
};

// Special case for byte arrays
template<> struct bsatn_type_id<std::vector<uint8_t>> { 
    static constexpr bool is_primitive = true;
    static constexpr uint8_t value = static_cast<uint8_t>(bsatn::AlgebraicTypeTag::Array); 
};

// -----------------------------------------------------------------------------
// Field Descriptor System
// -----------------------------------------------------------------------------

// Field descriptor for runtime reflection
struct FieldDescriptor {
    std::string name;
    size_t offset;
    size_t size;
    std::function<void(std::vector<uint8_t>&)> write_type;      // Writes AlgebraicType (legacy)
    std::function<bsatn::AlgebraicType()> get_algebraic_type;   // Returns AlgebraicType for type registry
    std::function<void(std::vector<uint8_t>&, const void*)> serialize;  // Serializes value
    std::function<std::string()> get_type_name;  // Returns the type name for complex types
};

// Table descriptor
struct TableDescriptor {
    std::vector<FieldDescriptor> fields;
};

// Global registry for table descriptors
inline std::map<const std::type_info*, TableDescriptor>& get_table_descriptors() {
    static std::map<const std::type_info*, TableDescriptor> descriptors;
    return descriptors;
}

// -----------------------------------------------------------------------------
// Type Writing Utilities
// -----------------------------------------------------------------------------

// Forward declaration
template<typename T>
void write_field_type(std::vector<uint8_t>& buf);

// Utility functions for writing BSATN data
inline void write_u32(std::vector<uint8_t>& buf, uint32_t val) {
    buf.push_back(val & 0xFF);
    buf.push_back((val >> 8) & 0xFF);
    buf.push_back((val >> 16) & 0xFF);
    buf.push_back((val >> 24) & 0xFF);
}

inline void write_string(std::vector<uint8_t>& buf, const std::string& str) {
    write_u32(buf, str.length());
    buf.insert(buf.end(), str.begin(), str.end());
}

// Unified type writer using modern C++ features
template<typename T>
void write_field_type(std::vector<uint8_t>& buf) {
    using Tag = bsatn::AlgebraicTypeTag;
    
    if constexpr (std::is_enum_v<T>) {
        // Enums are serialized as u32
        buf.push_back(static_cast<uint8_t>(Tag::U32));
    } 
    else if constexpr (bsatn_type_id<T>::is_primitive) {
        // Primitive types
        buf.push_back(bsatn_type_id<T>::value);
    }
    else if constexpr (is_vector<T>::value) {
        // Vector types
        buf.push_back(static_cast<uint8_t>(Tag::Array));
        write_field_type<typename T::value_type>(buf);
    }
    else if constexpr (is_optional<T>::value) {
        // Optional types
        buf.push_back(static_cast<uint8_t>(Tag::Sum));
        write_u32(buf, 2);  // 2 variants
        
        // Variant 0: Some
        buf.push_back(0);  // Some tag
        write_string(buf, "some");
        write_field_type<typename T::value_type>(buf);
        
        // Variant 1: None
        buf.push_back(0);  // Some tag
        write_string(buf, "none");
        buf.push_back(static_cast<uint8_t>(Tag::Product));  // Unit type
        write_u32(buf, 0);  // 0 fields
    }
    else if constexpr (std::is_same_v<T, Identity>) {
        // Identity is array of 32 bytes
        buf.push_back(static_cast<uint8_t>(Tag::Array));
        buf.push_back(static_cast<uint8_t>(Tag::U8));
    }
    else {
        // Custom struct types
        auto& descriptors = get_table_descriptors();
        auto it = descriptors.find(&typeid(T));
        
        if (it != descriptors.end()) {
            // Write as Product type
            buf.push_back(static_cast<uint8_t>(Tag::Product));
            write_u32(buf, it->second.fields.size());
            
            for (const auto& field : it->second.fields) {
                buf.push_back(0);  // Some (field name present)
                write_string(buf, field.name);
                field.write_type(buf);
            }
        } else {
            // Fallback - empty product
            buf.push_back(static_cast<uint8_t>(Tag::Product));
            write_u32(buf, 0);
        }
    }
}

// Universal serialize_value function using BSATN
template<typename T>
void serialize_value(std::vector<uint8_t>& buf, const T& val) {
    bsatn::Writer writer;
    bsatn::serialize(writer, val);
    const auto& serialized = writer.get_buffer();
    buf.insert(buf.end(), serialized.begin(), serialized.end());
}

// -----------------------------------------------------------------------------
// Helper Functions
// -----------------------------------------------------------------------------

// Forward declaration for recursive BSATN size calculation
template<typename T>
constexpr size_t calculate_bsatn_size();

// Helper function to get correct BSATN serialization size (not C++ sizeof with padding)
template<typename T>
constexpr size_t get_field_size() {
    // CRITICAL: Unit types should have size = 0, not sizeof() = 1
    if constexpr (requires { T::__is_unit_type__; } && T::__is_unit_type__) {
        return 0;  // Unit types serialize as 0 bytes
    } else if constexpr (std::is_same_v<T, std::monostate>) {
        return 0;  // std::monostate is also a unit type
    } else {
        // For complex types, calculate actual BSATN size, not C++ sizeof()
        return calculate_bsatn_size<T>();
    }
}

// Calculate BSATN serialization size for any type
template<typename T>
constexpr size_t calculate_bsatn_size() {
    // Unit types
    if constexpr ((requires { T::__is_unit_type__; } && T::__is_unit_type__) || 
                  std::is_same_v<T, std::monostate>) {
        return 0;
    }
    // Primitive types - use their natural sizes (no padding in BSATN)
    else if constexpr (std::is_same_v<T, bool> || std::is_same_v<T, uint8_t> || std::is_same_v<T, int8_t>) {
        return 1;
    }
    else if constexpr (std::is_same_v<T, uint16_t> || std::is_same_v<T, int16_t>) {
        return 2;
    }
    else if constexpr (std::is_same_v<T, uint32_t> || std::is_same_v<T, int32_t> || std::is_same_v<T, float>) {
        return 4;
    }
    else if constexpr (std::is_same_v<T, uint64_t> || std::is_same_v<T, int64_t> || std::is_same_v<T, double>) {
        return 8;
    }
    // For complex types (structs), we'd need runtime field information
    // For now, fall back to sizeof() but this should be improved
    else {
        return sizeof(T);
    }
}

// -----------------------------------------------------------------------------
// Field Registration Macros
// -----------------------------------------------------------------------------

// Primary macro for registering a field with auto-initialization
#define REGISTER_FIELD(struct_type, field_name, field_type) \
    __attribute__((export_name("__preinit__10_field_" #struct_type "_" #field_name))) \
    extern "C" void CONCAT(_preinit_register_field_, CONCAT(struct_type, field_name))() { \
        SpacetimeDB::FieldDescriptor desc; \
        desc.name = #field_name; \
        desc.offset = offsetof(struct_type, field_name); \
        desc.size = SpacetimeDB::get_field_size<field_type>(); \
        desc.write_type = [](std::vector<uint8_t>& buf) { \
            SpacetimeDB::write_field_type<field_type>(buf); \
        }; \
        desc.get_algebraic_type = []() { \
            return SpacetimeDB::bsatn::bsatn_traits<field_type>::algebraic_type(); \
        }; \
        desc.serialize = [](std::vector<uint8_t>& buf, const void* obj) { \
            const struct_type* typed_obj = static_cast<const struct_type*>(obj); \
            SpacetimeDB::serialize_value(buf, typed_obj->field_name); \
        }; \
        desc.get_type_name = []() -> std::string { \
            /* UNIFIED REGISTRY: Type names handled by demangling */ \
            return demangle_cpp_type_name(typeid(field_type).name()); \
        }; \
        SpacetimeDB::get_table_descriptors()[&typeid(struct_type)].fields.push_back(desc); \
    }

// -----------------------------------------------------------------------------
// Field Registrar Template
// -----------------------------------------------------------------------------

// Template for on-demand field registration - used by Module_impl.h
template<typename T>
struct field_registrar {
    static void register_fields() {
        // Default implementation - no fields to register
        // Specialized by SPACETIMEDB_STRUCT macro in traits.h
    }
};

// Specialization to prevent direct registration of std::variant as table types
template<typename... Ts>
struct field_registrar<std::variant<Ts...>> {
    static void register_fields() {
        static_assert(sizeof...(Ts) == 0, 
            "std::variant types cannot be used directly. "
            "Use SPACETIMEDB_VARIANT_ENUM macro to create variant types with named variants. "
            "Example: SPACETIMEDB_VARIANT_ENUM(MyEnum, (VariantA, uint32_t), (VariantB, std::string))");
        
        // This should never execute due to static_assert, but provide runtime error as backup
        std::abort(); // Cannot use std::variant directly
    }
};

// -----------------------------------------------------------------------------
// Legacy Support
// -----------------------------------------------------------------------------
// These are kept for backward compatibility but should not be used in new code.
// Use SPACETIMEDB_STRUCT from traits.h instead.

// Use unified macro system from macros.h

} // namespace SpacetimeDB

#endif // SPACETIMEDB_FIELD_REGISTRATION_H