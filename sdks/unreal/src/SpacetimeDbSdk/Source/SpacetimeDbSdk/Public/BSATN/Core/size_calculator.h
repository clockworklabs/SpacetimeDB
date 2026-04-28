#ifndef SPACETIMEDB_BSATN_SIZE_CALCULATOR_H
#define SPACETIMEDB_BSATN_SIZE_CALCULATOR_H

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>
#include <optional>
#include <type_traits>
#include <array>
#include "writer.h"  // For Writer

namespace SpacetimeDb::bsatn {

// Forward declaration
template<typename T> struct bsatn_traits;

/**
 * A Writer-compatible class that only counts bytes without storing them.
 * Used for size calculation via bsatn_traits::serialize().
 */
class SizeWriter {
private:
    size_t size_ = 0;

public:
    size_t size() const { return size_; }
    
    // Writer interface implementation
    void write_u8(uint8_t) { size_ += 1; }
    void write_u16_le(uint16_t) { size_ += 2; }
    void write_u32_le(uint32_t) { size_ += 4; }
    void write_u64_le(uint64_t) { size_ += 8; }
    void write_u128_le(const std::array<uint8_t, 16>&) { size_ += 16; }
    void write_u256_le(const std::array<uint8_t, 32>&) { size_ += 32; }
    
    void write_i8(int8_t) { size_ += 1; }
    void write_i16_le(int16_t) { size_ += 2; }
    void write_i32_le(int32_t) { size_ += 4; }
    void write_i64_le(int64_t) { size_ += 8; }
    void write_i128_le(const std::array<uint8_t, 16>&) { size_ += 16; }
    void write_i256_le(const std::array<uint8_t, 32>&) { size_ += 32; }
    
    void write_f32_le(float) { size_ += 4; }
    void write_f64_le(double) { size_ += 8; }
    
    void write_bool(bool) { size_ += 1; }
    
    void write_string(const std::string& s) {
        size_ += 4 + s.length();  // Length prefix + data
    }
    
    void write_bytes(const std::vector<uint8_t>& bytes) {
        size_ += 4 + bytes.size();  // Length prefix + data
    }
    
    void write_bytes(const void*, size_t len) {
        size_ += 4 + len;  // Length prefix + data
    }
    
    // No-op methods for size calculation
    std::vector<uint8_t> take_buffer() { return {}; }
    void clear() { size_ = 0; }
};

/**
 * Calculate the serialized size of values without actually serializing.
 * This matches Rust's CountWriter functionality.
 */
class SizeCalculator {
private:
    size_t size_ = 0;
    
public:
    size_t size() const { return size_; }
    
    void add_bool() { size_ += 1; }
    void add_u8() { size_ += 1; }
    void add_u16() { size_ += 2; }
    void add_u32() { size_ += 4; }
    void add_u64() { size_ += 8; }
    void add_u128() { size_ += 16; }
    void add_u256() { size_ += 32; }
    
    void add_i8() { size_ += 1; }
    void add_i16() { size_ += 2; }
    void add_i32() { size_ += 4; }
    void add_i64() { size_ += 8; }
    void add_i128() { size_ += 16; }
    void add_i256() { size_ += 32; }
    
    void add_f32() { size_ += 4; }
    void add_f64() { size_ += 8; }
    
    void add_string(const std::string& s) {
        size_ += 4 + s.length();  // Length prefix + data
    }
    
    void add_bytes(size_t len) {
        size_ += 4 + len;  // Length prefix + data
    }
    
    template<typename T>
    void add_vector(const std::vector<T>& vec) {
        size_ += 4;  // Length prefix
        for (const auto& item : vec) {
            add_value(item);
        }
    }
    
    template<typename T>
    void add_optional(const std::optional<T>& opt) {
        size_ += 1;  // Tag byte
        if (opt.has_value()) {
            add_value(*opt);
        }
    }
    
    // Generic value addition
    template<typename T>
    void add_value(const T& value);
};

/**
 * Trait to determine if a type has a static (compile-time known) size.
 * This matches Rust's static_bsatn_size functionality.
 */
template<typename T>
struct has_static_size : std::false_type {};

template<> struct has_static_size<bool> : std::true_type { static constexpr size_t value = 1; };
template<> struct has_static_size<uint8_t> : std::true_type { static constexpr size_t value = 1; };
template<> struct has_static_size<uint16_t> : std::true_type { static constexpr size_t value = 2; };
template<> struct has_static_size<uint32_t> : std::true_type { static constexpr size_t value = 4; };
template<> struct has_static_size<uint64_t> : std::true_type { static constexpr size_t value = 8; };
template<> struct has_static_size<int8_t> : std::true_type { static constexpr size_t value = 1; };
template<> struct has_static_size<int16_t> : std::true_type { static constexpr size_t value = 2; };
template<> struct has_static_size<int32_t> : std::true_type { static constexpr size_t value = 4; };
template<> struct has_static_size<int64_t> : std::true_type { static constexpr size_t value = 8; };
template<> struct has_static_size<float> : std::true_type { static constexpr size_t value = 4; };
template<> struct has_static_size<double> : std::true_type { static constexpr size_t value = 8; };

template<typename T>
inline constexpr bool has_static_size_v = has_static_size<T>::value;

template<typename T>
inline constexpr size_t static_size_v = has_static_size<T>::value;

/**
 * Calculate the BSATN serialized size of a value.
 * Similar to Rust's to_len() function.
 */
template<typename T>
size_t bsatn_len(const T& value) {
    if constexpr (has_static_size_v<T>) {
        return static_size_v<T>;
    } else {
        SizeCalculator calc;
        calc.add_value(value);
        return calc.size();
    }
}

/**
 * Optimized serialization that pre-allocates the exact size needed.
 * Similar to Rust's to_bsatn_vec().
 */
template<typename T>
std::vector<uint8_t> to_bsatn_vec(const T& value) {
    size_t size = bsatn_len(value);
    std::vector<uint8_t> result;
    result.reserve(size);
    
    // Use existing Writer to serialize
    Writer writer;
    serialize(writer, value);
    return writer.take_buffer();
}

/**
 * Extend an existing vector with BSATN data.
 * Similar to Rust's to_bsatn_extend().
 */
template<typename T>
void to_bsatn_extend(std::vector<uint8_t>& vec, const T& value) {
    size_t old_size = vec.size();
    size_t add_size = bsatn_len(value);
    vec.reserve(old_size + add_size);
    
    // Serialize directly into the vector
    // (Would need custom Writer that appends to existing vector)
    auto temp = to_bsatn_vec(value);
    vec.insert(vec.end(), temp.begin(), temp.end());
}

/**
 * Type trait to check if a type is primitive (no padding).
 * Similar to Rust's IsPrimitiveType.
 */
template<typename T>
struct is_primitive_type : std::false_type {};

template<> struct is_primitive_type<bool> : std::true_type {};
template<> struct is_primitive_type<uint8_t> : std::true_type {};
template<> struct is_primitive_type<uint16_t> : std::true_type {};
template<> struct is_primitive_type<uint32_t> : std::true_type {};
template<> struct is_primitive_type<uint64_t> : std::true_type {};
template<> struct is_primitive_type<int8_t> : std::true_type {};
template<> struct is_primitive_type<int16_t> : std::true_type {};
template<> struct is_primitive_type<int32_t> : std::true_type {};
template<> struct is_primitive_type<int64_t> : std::true_type {};
template<> struct is_primitive_type<float> : std::true_type {};
template<> struct is_primitive_type<double> : std::true_type {};

template<typename T>
inline constexpr bool is_primitive_type_v = is_primitive_type<T>::value;

// Implementation of SizeCalculator::add_value
template<typename T>
void SizeCalculator::add_value(const T& value) {
    if constexpr (std::is_same_v<T, bool>) {
        add_bool();
    } else if constexpr (std::is_same_v<T, uint8_t>) {
        add_u8();
    } else if constexpr (std::is_same_v<T, uint16_t>) {
        add_u16();
    } else if constexpr (std::is_same_v<T, uint32_t>) {
        add_u32();
    } else if constexpr (std::is_same_v<T, uint64_t>) {
        add_u64();
    } else if constexpr (std::is_same_v<T, int8_t>) {
        add_i8();
    } else if constexpr (std::is_same_v<T, int16_t>) {
        add_i16();
    } else if constexpr (std::is_same_v<T, int32_t>) {
        add_i32();
    } else if constexpr (std::is_same_v<T, int64_t>) {
        add_i64();
    } else if constexpr (std::is_same_v<T, float>) {
        add_f32();
    } else if constexpr (std::is_same_v<T, double>) {
        add_f64();
    } else if constexpr (std::is_same_v<T, std::string>) {
        add_string(value);
    } else {
        // For custom types, use a SizeWriter to calculate size via bsatn_traits
        SizeWriter size_writer;
        bsatn_traits<T>::serialize(size_writer, value);
        size_ += size_writer.size();
    }
}

} // namespace SpacetimeDb::bsatn

#endif // SPACETIMEDB_BSATN_SIZE_CALCULATOR_H