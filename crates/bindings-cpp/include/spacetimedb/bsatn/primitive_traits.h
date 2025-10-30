#ifndef SPACETIMEDB_BSATN_PRIMITIVE_TRAITS_H
#define SPACETIMEDB_BSATN_PRIMITIVE_TRAITS_H

/**
 * @file primitive_traits.h
 * @brief BSATN trait specializations for primitive types
 * 
 * This file provides bsatn_traits specializations for all primitive types
 * supported by SpacetimeDB. These specializations delegate to the existing
 * serialize functions in writer.h and deserializer struct in reader.h.
 */

#include "traits.h"
#include "reader.h"
#include "writer.h"
#include "algebraic_type.h"
#include <string>
#include <type_traits>

namespace SpacetimeDb::bsatn {

// Forward declarations of serialize functions from writer.h
inline void serialize(Writer& w, bool value);
inline void serialize(Writer& w, uint8_t value);
inline void serialize(Writer& w, uint16_t value);
inline void serialize(Writer& w, uint32_t value);
inline void serialize(Writer& w, uint64_t value);
inline void serialize(Writer& w, int8_t value);
inline void serialize(Writer& w, int16_t value);
inline void serialize(Writer& w, int32_t value);
inline void serialize(Writer& w, int64_t value);
inline void serialize(Writer& w, float value);
inline void serialize(Writer& w, double value);
inline void serialize(Writer& w, const std::string& value);

// =========================================================================
// Boolean Type
// =========================================================================

template<>
struct bsatn_traits<bool> {
    static void serialize(Writer& writer, bool value) {
        writer.write_bool(value);
    }
    
    static bool deserialize(Reader& reader) {
        return reader.read_bool();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::Bool();
    }
};

// =========================================================================
// Signed Integer Types
// =========================================================================

template<>
struct bsatn_traits<int8_t> {
    static void serialize(Writer& writer, int8_t value) {
        writer.write_i8(value);
    }
    
    static int8_t deserialize(Reader& reader) {
        return reader.read_i8();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::I8();
    }
};

template<>
struct bsatn_traits<int16_t> {
    static void serialize(Writer& writer, int16_t value) {
        writer.write_i16_le(value);
    }
    
    static int16_t deserialize(Reader& reader) {
        return reader.read_i16_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::I16();
    }
};

template<>
struct bsatn_traits<int32_t> {
    static void serialize(Writer& writer, int32_t value) {
        writer.write_i32_le(value);
    }
    
    static int32_t deserialize(Reader& reader) {
        return reader.read_i32_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::I32();
    }
};

template<>
struct bsatn_traits<int64_t> {
    static void serialize(Writer& writer, int64_t value) {
        writer.write_i64_le(value);
    }
    
    static int64_t deserialize(Reader& reader) {
        return reader.read_i64_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::I64();
    }
};

// =========================================================================
// Unsigned Integer Types
// =========================================================================

template<>
struct bsatn_traits<uint8_t> {
    static void serialize(Writer& writer, uint8_t value) {
        writer.write_u8(value);
    }
    
    static uint8_t deserialize(Reader& reader) {
        return reader.read_u8();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U8();
    }
};

template<>
struct bsatn_traits<uint16_t> {
    static void serialize(Writer& writer, uint16_t value) {
        writer.write_u16_le(value);
    }
    
    static uint16_t deserialize(Reader& reader) {
        return reader.read_u16_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U16();
    }
};

template<>
struct bsatn_traits<uint32_t> {
    static void serialize(Writer& writer, uint32_t value) {
        writer.write_u32_le(value);
    }
    
    static uint32_t deserialize(Reader& reader) {
        return reader.read_u32_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U32();
    }
};

template<>
struct bsatn_traits<uint64_t> {
    static void serialize(Writer& writer, uint64_t value) {
        writer.write_u64_le(value);
    }
    
    static uint64_t deserialize(Reader& reader) {
        return reader.read_u64_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U64();
    }
};

// =========================================================================
// Floating Point Types
// =========================================================================

template<>
struct bsatn_traits<float> {
    static void serialize(Writer& writer, float value) {
        writer.write_f32_le(value);
    }
    
    static float deserialize(Reader& reader) {
        return reader.read_f32_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::F32();
    }
};

template<>
struct bsatn_traits<double> {
    static void serialize(Writer& writer, double value) {
        writer.write_f64_le(value);
    }
    
    static double deserialize(Reader& reader) {
        return reader.read_f64_le();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::F64();
    }
};

// =========================================================================
// String Type
// =========================================================================

template<>
struct bsatn_traits<std::string> {
    static void serialize(Writer& writer, const std::string& value) {
        writer.write_string(value);
    }
    
    static std::string deserialize(Reader& reader) {
        return reader.read_string();
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::String();
    }
};

// Note: Platform-specific type aliases (like 'int' vs 'int32_t') are handled
// by the existing type system. On most platforms, 'int' is an alias for 'int32_t'
// and 'unsigned int' is an alias for 'uint32_t', so they use the same specializations.

// =========================================================================
// Generic Enum Type Support
// =========================================================================

/**
 * Generic enum trait specialization that works for any enum type.
 * Enums are serialized as their underlying type (typically uint32_t).
 * 
 * Usage:
 *   enum class MyEnum : uint8_t { Zero, One, Two };
 *   // Automatically supported for BSATN serialization
 */
template<typename T>
requires std::is_enum_v<T>
struct bsatn_traits<T> {
    static void serialize(Writer& writer, const T& value) {
        using underlying = std::underlying_type_t<T>;
        // Delegate to the underlying type's serialization
        bsatn_traits<underlying>::serialize(writer, static_cast<underlying>(value));
    }
    
    static T deserialize(Reader& reader) {
        using underlying = std::underlying_type_t<T>;
        // Delegate to the underlying type's deserialization
        return static_cast<T>(bsatn_traits<underlying>::deserialize(reader));
    }
    
    static AlgebraicType algebraic_type() {
        using underlying = std::underlying_type_t<T>;
        return bsatn_traits<underlying>::algebraic_type();
    }
};

} // namespace SpacetimeDb::bsatn

#endif // SPACETIMEDB_BSATN_PRIMITIVE_TRAITS_H