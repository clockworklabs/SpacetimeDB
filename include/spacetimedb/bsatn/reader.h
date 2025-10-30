#ifndef SPACETIMEDB_BSATN_READER_H
#define SPACETIMEDB_BSATN_READER_H

#include <vector>
#include <string>
#include <cstdint>
#include <stdexcept>
#include <optional>
#include <functional>
#include <span>
#include <type_traits>
#include <cstring>
#include <variant>
// uint128_placeholder.h removed - types are in spacetimedb/types.h
#include "types.h"

namespace SpacetimeDb::bsatn {

    class Reader;
    template<typename T> T deserialize(Reader& r);
    template<typename T> struct bsatn_traits;

    // Helper traits
    template<typename> struct is_std_optional : std::false_type {};
    template<typename T> struct is_std_optional<std::optional<T>> : std::true_type {};
    template<typename T> constexpr bool is_std_optional_v = is_std_optional<T>::value;

    template<typename> struct is_std_vector : std::false_type {};
    template<typename T> struct is_std_vector<std::vector<T>> : std::true_type {};
    template<typename T> constexpr bool is_std_vector_v = is_std_vector<T>::value;

    class Reader {
    public:
        // Constructors - using uint8_t consistently
        Reader(const uint8_t* data, size_t size) : current_ptr(data), end_ptr(data + size) {}
        Reader(std::span<const uint8_t> data) : current_ptr(data.data()), end_ptr(data.data() + data.size()) {}
        Reader(const std::vector<uint8_t>& data) : current_ptr(data.data()), end_ptr(data.data() + data.size()) {}
        

        // Template method for reading primitive types (reduces implementation duplication)
        template<typename T>
        T read_primitive_le() {
            static_assert(std::is_arithmetic_v<T>, "read_primitive_le only works with arithmetic types");
            check_available(sizeof(T));
            T val;
            std::memcpy(&val, current_ptr, sizeof(T));
            advance(sizeof(T));
            return val;
        }

        // Public API methods (delegates to template where possible)
        inline bool read_bool() {
            check_available(1);
            uint8_t val = *current_ptr;
            advance(1);
            if (val > 1) {
                std::abort(); // Invalid bool value in BSATN deserialization
            }
            return val != 0;
        }
        inline uint8_t read_u8() {
            check_available(1);
            uint8_t val = *current_ptr;
            advance(1);
            return val;
        }
        uint16_t read_u16_le() { return read_primitive_le<uint16_t>(); }
        uint32_t read_u32_le() { return read_primitive_le<uint32_t>(); }
        uint64_t read_u64_le() { return read_primitive_le<uint64_t>(); }
        inline SpacetimeDb::u128 read_u128_le() {
            uint64_t low = read_u64_le();
            uint64_t high = read_u64_le();
            return SpacetimeDb::u128(high, low);
        }
        inline SpacetimeDb::u256_placeholder read_u256_le() {
            check_available(32);
            SpacetimeDb::u256_placeholder val;
            std::memcpy(val.data.data(), current_ptr, 32);
            advance(32);
            return val;
        }

        int8_t read_i8() { return static_cast<int8_t>(read_u8()); }
        int16_t read_i16_le() { return static_cast<int16_t>(read_u16_le()); }
        int32_t read_i32_le() { return static_cast<int32_t>(read_u32_le()); }
        int64_t read_i64_le() { return static_cast<int64_t>(read_u64_le()); }
        inline SpacetimeDb::i128 read_i128_le() {
            uint64_t low = read_u64_le();
            int64_t high = static_cast<int64_t>(read_u64_le());
            return SpacetimeDb::i128(high, low);
        }
        inline SpacetimeDb::i256_placeholder read_i256_le() {
            check_available(32);
            SpacetimeDb::i256_placeholder val;
            std::memcpy(val.data.data(), current_ptr, 32);
            advance(32);
            return val;
        }

        float read_f32_le() { return read_primitive_le<float>(); }
        double read_f64_le() { return read_primitive_le<double>(); }

        inline std::string read_string() {
            uint32_t len = read_u32_le();
            check_available(len);
            std::string result(reinterpret_cast<const char*>(current_ptr), len);
            advance(len);
            return result;
        }
        inline std::vector<uint8_t> read_bytes() {
            uint32_t len = read_u32_le();
            check_available(len);
            std::vector<uint8_t> result(current_ptr, current_ptr + len);
            advance(len);
            return result;
        }
        inline std::vector<uint8_t> read_fixed_bytes(size_t count) {
            check_available(count);
            std::vector<uint8_t> result(current_ptr, current_ptr + count);
            advance(count);
            return result;
        }

        template<typename T>
        std::optional<T> read_optional() {
            uint8_t tag = read_u8();
            if (tag == 0) {
                return std::nullopt;
            } else if (tag == 1) {
                return SpacetimeDb::bsatn::deserialize<T>(*this);
            } else {
                std::abort(); // Invalid optional tag in BSATN deserialization
            }
        }

        template<typename T>
        std::vector<T> read_vector() {
            uint32_t size = read_u32_le();
            std::vector<T> result;
            result.reserve(size);
            for (uint32_t i = 0; i < size; ++i) {
                result.push_back(SpacetimeDb::bsatn::deserialize<T>(*this));
            }
            return result;
        }

        inline std::vector<uint8_t> read_vector_byte() {
            return read_bytes();
        }

        // Deserialize a type using C++20 concepts for better error messages
        template<typename T>
            requires requires(Reader& r) { SpacetimeDb::bsatn::deserialize<T>(r); }
        T deserialize_type() {
            return SpacetimeDb::bsatn::deserialize<T>(*this);
        }

        inline bool is_eos() const {
            return current_ptr >= end_ptr;
        }
        inline size_t remaining_bytes() const {
            return (current_ptr <= end_ptr) ? (end_ptr - current_ptr) : 0;
        }

    private:
        inline void check_available(size_t num_bytes) const {
            if (current_ptr + num_bytes > end_ptr) {
                std::abort(); // BSATN Reader: Not enough bytes remaining
            }
        }
        inline void advance(size_t num_bytes) {
            current_ptr += num_bytes;
        }

        const uint8_t* current_ptr;
        const uint8_t* end_ptr;
    };

    // Type trait for deserializing types - primary template
    template<typename T, typename = void>
    struct deserializer {
        static T deserialize(Reader& r) {
            // Default: try bsatn_traits
            return bsatn_traits<T>::deserialize(r);
        }
    };
    
    // Specializations for primitive types
    template<> struct deserializer<bool> {
        static bool deserialize(Reader& r) { return r.read_bool(); }
    };
    template<> struct deserializer<uint8_t> {
        static uint8_t deserialize(Reader& r) { return r.read_u8(); }
    };
    template<> struct deserializer<uint16_t> {
        static uint16_t deserialize(Reader& r) { return r.read_u16_le(); }
    };
    template<> struct deserializer<uint32_t> {
        static uint32_t deserialize(Reader& r) { return r.read_u32_le(); }
    };
    template<> struct deserializer<uint64_t> {
        static uint64_t deserialize(Reader& r) { return r.read_u64_le(); }
    };
    template<> struct deserializer<int8_t> {
        static int8_t deserialize(Reader& r) { return r.read_i8(); }
    };
    template<> struct deserializer<int16_t> {
        static int16_t deserialize(Reader& r) { return r.read_i16_le(); }
    };
    template<> struct deserializer<int32_t> {
        static int32_t deserialize(Reader& r) { return r.read_i32_le(); }
    };
    template<> struct deserializer<int64_t> {
        static int64_t deserialize(Reader& r) { return r.read_i64_le(); }
    };
    template<> struct deserializer<float> {
        static float deserialize(Reader& r) { return r.read_f32_le(); }
    };
    template<> struct deserializer<double> {
        static double deserialize(Reader& r) { return r.read_f64_le(); }
    };
    template<> struct deserializer<std::string> {
        static std::string deserialize(Reader& r) { return r.read_string(); }
    };
    template<> struct deserializer<std::vector<uint8_t>> {
        static std::vector<uint8_t> deserialize(Reader& r) { return r.read_bytes(); }
    };
    
    // Specializations for container types
    template<typename T>
    struct deserializer<std::optional<T>> {
        static std::optional<T> deserialize(Reader& r) {
            return r.read_optional<T>();
        }
    };
    
    template<typename T>
    struct deserializer<std::vector<T>, std::enable_if_t<!std::is_same_v<T, uint8_t>>> {
        static std::vector<T> deserialize(Reader& r) {
            return r.read_vector<T>();
        }
    };
    
    // Specializations for SpacetimeDB types
    template<> struct deserializer<SpacetimeDb::Identity> {
        static SpacetimeDb::Identity deserialize(Reader& r) {
            SpacetimeDb::Identity id;
            id.bsatn_deserialize(r);
            return id;
        }
    };
    template<> struct deserializer<SpacetimeDb::ConnectionId> {
        static SpacetimeDb::ConnectionId deserialize(Reader& r) {
            SpacetimeDb::ConnectionId conn;
            conn.bsatn_deserialize(r);
            return conn;
        }
    };
    
    // Generic deserialize function - now much cleaner!
    template<typename T>
    inline T deserialize(Reader& r) {
        return deserializer<T>::deserialize(r);
    }

} // namespace SpacetimeDB::bsatn

#endif // SPACETIMEDB_BSATN_READER_H