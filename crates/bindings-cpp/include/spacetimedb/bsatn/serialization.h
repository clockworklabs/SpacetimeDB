#ifndef SPACETIMEDB_BSATN_SERIALIZATION_H
#define SPACETIMEDB_BSATN_SERIALIZATION_H

/**
 * @file serialization.h
 * @brief Core serialization and deserialization functions for BSATN
 * 
 * This header provides the main entry points for BSATN serialization
 * using C++20 concepts for better type safety and error messages.
 */

#include "traits.h"
#include "reader.h"
#include "writer.h"
#include "size_calculator.h"
#include <concepts>
#include <type_traits>

namespace SpacetimeDb::bsatn {

/**
 * @brief Concept for types that can be serialized to BSATN
 */
template<typename T>
concept Serializable = requires(Writer& w, const T& v) {
    bsatn_traits<T>::serialize(w, v);
};

/**
 * @brief Concept for types that can be deserialized from BSATN
 */
template<typename T>
concept Deserializable = requires(Reader& r) {
    { bsatn_traits<T>::deserialize(r) } -> std::same_as<T>;
};

/**
 * @brief Serialize a value to BSATN format
 * 
 * @tparam T Type to serialize (must satisfy Serializable concept)
 * @param writer The writer to serialize to
 * @param value The value to serialize
 * 
 * @example
 * @code
 * Writer writer;
 * MyStruct data{42, "hello"};
 * serialize(writer, data);
 * auto bytes = writer.take_buffer();
 * @endcode
 */
template<Serializable T>
inline void serialize(Writer& writer, const T& value) {
    bsatn_traits<T>::serialize(writer, value);
}

/**
 * @brief Deserialize a value from BSATN format
 * 
 * @tparam T Type to deserialize (must satisfy Deserializable concept)
 * @param reader The reader to deserialize from
 * @return The deserialized value
 * 
 * @example
 * @code
 * Reader reader(bytes);
 * auto data = deserialize<MyStruct>(reader);
 * @endcode
 */
template<Deserializable T>
inline T deserialize(Reader& reader) {
    return bsatn_traits<T>::deserialize(reader);
}

/**
 * @brief Serialize multiple values at once
 * 
 * Uses C++20 parameter packs with concepts for type safety.
 * 
 * @example
 * @code
 * Writer writer;
 * serialize_all(writer, 42, "hello", true, 3.14);
 * @endcode
 */
template<typename... Args>
    requires (Serializable<Args> && ...)
inline void serialize_all(Writer& writer, const Args&... args) {
    (serialize(writer, args), ...);
}

/**
 * @brief Helper to serialize to a byte vector
 * 
 * @tparam T Type to serialize
 * @param value The value to serialize
 * @return Vector of bytes containing serialized data
 */
template<Serializable T>
inline std::vector<uint8_t> to_bytes(const T& value) {
    Writer writer;
    serialize(writer, value);
    return writer.take_buffer();
}

/**
 * @brief Helper to deserialize from a byte vector
 * 
 * @tparam T Type to deserialize
 * @param bytes The byte vector to deserialize from
 * @return The deserialized value
 */
template<Deserializable T>
inline T from_bytes(const std::vector<uint8_t>& bytes) {
    Reader reader(bytes);
    return deserialize<T>(reader);
}

// Note: size_calculator.h must be included for these concepts to work
// It defines has_static_size<T> and static_size_v<T>

/**
 * @brief Concept for types that have a static size
 * 
 * This requires that size_calculator.h has been included to provide
 * the has_static_size template.
 */
template<typename T>
concept HasStaticSize = requires {
    typename std::enable_if_t<std::is_class_v<has_static_size<T>>>;
    { has_static_size<T>::value } -> std::convertible_to<bool>;
} && has_static_size<T>::value == true;

/**
 * @brief Get the static BSATN size of a type at compile time
 * 
 * @tparam T Type with static size
 * @return The size in bytes
 */
template<typename T>
    requires HasStaticSize<T>
consteval size_t static_bsatn_size() {
    return has_static_size<T>::value;
}

} // namespace SpacetimeDb::bsatn

#endif // SPACETIMEDB_BSATN_SERIALIZATION_H