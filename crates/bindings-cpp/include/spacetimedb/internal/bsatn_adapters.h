#pragma once

#include <spacetimedb/bsatn/writer.h>
#include <spacetimedb/bsatn/reader.h>
#include <spacetimedb/abi/FFI.h>
#include <vector>
#include <cstdint>

namespace SpacetimeDB {
namespace Internal {

/**
 * Adapter class that allows using BSATN Reader with BytesSource
 * 
 * This is needed because FFI uses BytesSource handles while our
 * BSATN system uses Reader objects. This adapter bridges the gap.
 */
class BytesSourceReader {
private:
    BytesSource source_;
    
public:
    explicit BytesSourceReader(BytesSource source) : source_(source) {}
    
    void read_bytes(uint8_t* buffer, size_t len) {
        size_t bytes_read = len;
        FFI::bytes_source_read(source_, buffer, &bytes_read);
        if (bytes_read != len) {
            std::abort(); // Failed to read expected number of bytes
        }
    }
    
    uint8_t read_u8() {
        uint8_t value;
        read_bytes(&value, 1);
        return value;
    }
    
    uint16_t read_u16_le() {
        uint8_t bytes[2];
        read_bytes(bytes, 2);
        return bytes[0] | (static_cast<uint16_t>(bytes[1]) << 8);
    }
    
    uint32_t read_u32_le() {
        uint8_t bytes[4];
        read_bytes(bytes, 4);
        return bytes[0] | 
               (static_cast<uint32_t>(bytes[1]) << 8) |
               (static_cast<uint32_t>(bytes[2]) << 16) |
               (static_cast<uint32_t>(bytes[3]) << 24);
    }
    
    uint64_t read_u64_le() {
        uint8_t bytes[8];
        read_bytes(bytes, 8);
        uint64_t result = 0;
        for (int i = 0; i < 8; ++i) {
            result |= static_cast<uint64_t>(bytes[i]) << (i * 8);
        }
        return result;
    }
    
    int8_t read_i8() { return static_cast<int8_t>(read_u8()); }
    int16_t read_i16_le() { return static_cast<int16_t>(read_u16_le()); }
    int32_t read_i32_le() { return static_cast<int32_t>(read_u32_le()); }
    int64_t read_i64_le() { return static_cast<int64_t>(read_u64_le()); }
    
    float read_f32_le() {
        uint32_t bits = read_u32_le();
        float value;
        std::memcpy(&value, &bits, sizeof(float));
        return value;
    }
    
    double read_f64_le() {
        uint64_t bits = read_u64_le();
        double value;
        std::memcpy(&value, &bits, sizeof(double));
        return value;
    }
    
    std::string read_string() {
        uint32_t len = read_u32_le();
        std::string result(len, '\0');
        read_bytes(reinterpret_cast<uint8_t*>(&result[0]), len);
        return result;
    }
    
    std::vector<uint8_t> read_fixed_bytes(size_t len) {
        std::vector<uint8_t> result(len);
        read_bytes(result.data(), len);
        return result;
    }
    
    // For compatibility with BSATN Reader interface
    bool read_bool() { return read_u8() != 0; }
    
    // Big integer support
    ::SpacetimeDB::u128 read_u128_le() {
        uint64_t low = read_u64_le();
        uint64_t high = read_u64_le();
        return ::SpacetimeDB::u128(high, low);
    }
    
    ::SpacetimeDB::i128 read_i128_le() {
        uint64_t low = read_u64_le();
        uint64_t high = read_u64_le();
        return ::SpacetimeDB::i128(static_cast<int64_t>(high), low);
    }
    
    ::SpacetimeDB::u256 read_u256_le() {
        ::SpacetimeDB::u256 result;
        read_bytes(result.data.data(), 32);
        return result;
    }
    
    ::SpacetimeDB::i256 read_i256_le() {
        ::SpacetimeDB::i256 result;
        read_bytes(result.data.data(), 32);
        return result;
    }
};

} // namespace Internal
} // namespace SpacetimeDB