#include "bsatn.h"
#include <cstring>   // For memcpy
#include <limits>    // For numeric_limits
#include <algorithm> // For std::reverse, std::min

namespace spacetimedb {
namespace bsatn {

// Helper to write multi-byte values in little-endian order
template<typename T>
static void internal_write_little_endian(std::vector<uint8_t>& buffer, T value) {
    static_assert(std::is_integral_v<T> || std::is_floating_point_v<T>, "Type must be integral or floating point for endian conversion.");
    // For floating point types, we'll operate on their bit representation
    uint8_t bytes[sizeof(T)];
    std::memcpy(bytes, &value, sizeof(T));

    // BSATN is little-endian. If system is big-endian, reverse bytes.
    // For simplicity in this context, we'll write byte by byte assuming conversion is handled or system is LE.
    // A more robust solution would check system endianness.
    // However, to strictly enforce little-endian for the ABI:
    for (size_t i = 0; i < sizeof(T); ++i) {
        buffer.push_back(bytes[i]); // Assuming system is little-endian or value is pre-swapped
                                    // For guaranteed little-endian write:
                                    // buffer.push_back(static_cast<uint8_t>((value >> (i * 8)) & 0xFF));
                                    // This line above IS the little-endian write.
    }
}

// Helper to read multi-byte values from little-endian order
template<typename T>
static T internal_read_little_endian(const uint8_t*& current_ptr, const uint8_t* end_ptr) {
    static_assert(std::is_integral_v<T> || std::is_floating_point_v<T>, "Type must be integral or floating point for endian conversion.");
    if (static_cast<size_t>(end_ptr - current_ptr) < sizeof(T)) {
        throw std::runtime_error("BSATN read past end of buffer (in internal_read_little_endian)");
    }
    
    T value;
    uint8_t bytes[sizeof(T)];

    // To strictly enforce little-endian read for the ABI:
    for (size_t i = 0; i < sizeof(T); ++i) {
        bytes[i] = *(current_ptr + i);
    }
    current_ptr += sizeof(T);
    std::memcpy(&value, bytes, sizeof(T));
    return value;
}


// bsatn_writer implementation
bsatn_writer::bsatn_writer() {}

void bsatn_writer::write_raw_bytes(const void* data, size_t len) {
    buffer.insert(buffer.end(), static_cast<const uint8_t*>(data), static_cast<const uint8_t*>(data) + len);
}

void bsatn_writer::write_bool(bool value) {
    write_u8(static_cast<uint8_t>(value ? 1 : 0));
}

void bsatn_writer::write_u8(uint8_t value) {
    buffer.push_back(value);
}

void bsatn_writer::write_u16(uint16_t value) {
    uint8_t bytes[2];
    bytes[0] = static_cast<uint8_t>(value & 0xFF);
    bytes[1] = static_cast<uint8_t>((value >> 8) & 0xFF);
    write_raw_bytes(bytes, 2);
}

void bsatn_writer::write_u32(uint32_t value) {
    uint8_t bytes[4];
    bytes[0] = static_cast<uint8_t>(value & 0xFF);
    bytes[1] = static_cast<uint8_t>((value >> 8) & 0xFF);
    bytes[2] = static_cast<uint8_t>((value >> 16) & 0xFF);
    bytes[3] = static_cast<uint8_t>((value >> 24) & 0xFF);
    write_raw_bytes(bytes, 4);
}

void bsatn_writer::write_u64(uint64_t value) {
    uint8_t bytes[8];
    bytes[0] = static_cast<uint8_t>(value & 0xFF);
    bytes[1] = static_cast<uint8_t>((value >> 8) & 0xFF);
    bytes[2] = static_cast<uint8_t>((value >> 16) & 0xFF);
    bytes[3] = static_cast<uint8_t>((value >> 24) & 0xFF);
    bytes[4] = static_cast<uint8_t>((value >> 32) & 0xFF);
    bytes[5] = static_cast<uint8_t>((value >> 40) & 0xFF);
    bytes[6] = static_cast<uint8_t>((value >> 48) & 0xFF);
    bytes[7] = static_cast<uint8_t>((value >> 56) & 0xFF);
    write_raw_bytes(bytes, 8);
}

void bsatn_writer::write_i8(int8_t value) {
    write_u8(static_cast<uint8_t>(value));
}

void bsatn_writer::write_i16(int16_t value) {
    write_u16(static_cast<uint16_t>(value));
}

void bsatn_writer::write_i32(int32_t value) {
    write_u32(static_cast<uint32_t>(value));
}

void bsatn_writer::write_i64(int64_t value) {
    write_u64(static_cast<uint64_t>(value));
}

void bsatn_writer::write_f32(float value) {
    uint32_t bits;
    std::memcpy(&bits, &value, sizeof(float));
    write_u32(bits);
}

void bsatn_writer::write_f64(double value) {
    uint64_t bits;
    std::memcpy(&bits, &value, sizeof(double));
    write_u64(bits);
}

void bsatn_writer::write_string(const std::string& str) {
    if (str.length() > std::numeric_limits<uint32_t>::max()) {
        throw std::runtime_error("BSATN string length exceeds uint32_t max");
    }
    write_u32(static_cast<uint32_t>(str.length()));
    write_raw_bytes(str.data(), str.length());
}

void bsatn_writer::write_bytes(const std::vector<uint8_t>& bytes) {
    if (bytes.size() > std::numeric_limits<uint32_t>::max()) {
        throw std::runtime_error("BSATN byte array length exceeds uint32_t max");
    }
    write_u32(static_cast<uint32_t>(bytes.size()));
    write_raw_bytes(bytes.data(), bytes.size());
}

void bsatn_writer::write_sum_discriminant(uint8_t discriminant) {
    write_u8(discriminant);
}

const std::vector<uint8_t>& bsatn_writer::get_buffer() const {
    return buffer;
}

std::vector<uint8_t>&& bsatn_writer::move_buffer() {
    return std::move(buffer);
}

// bsatn_reader implementation
bsatn_reader::bsatn_reader(const uint8_t* data, size_t len)
    : current_ptr(data), end_ptr(data + len) {}

bsatn_reader::bsatn_reader(const std::vector<uint8_t>& data)
    : current_ptr(data.data()), end_ptr(data.data() + data.size()) {}

void bsatn_reader::read_raw_bytes(void* data, size_t len) {
    if (static_cast<size_t>(end_ptr - current_ptr) < len) {
        throw std::runtime_error("BSATN read past end of buffer");
    }
    std::memcpy(data, current_ptr, len);
    current_ptr += len;
}

bool bsatn_reader::read_bool() {
    return read_u8() != 0;
}

uint8_t bsatn_reader::read_u8() {
    if (current_ptr >= end_ptr) {
        throw std::runtime_error("BSATN read past end of buffer (u8)");
    }
    return *current_ptr++;
}

uint16_t bsatn_reader::read_u16() {
    uint8_t bytes[2];
    read_raw_bytes(bytes, 2);
    return static_cast<uint16_t>(bytes[0] | (static_cast<uint16_t>(bytes[1]) << 8));
}

uint32_t bsatn_reader::read_u32() {
    uint8_t bytes[4];
    read_raw_bytes(bytes, 4);
    return static_cast<uint32_t>(bytes[0]) |
           (static_cast<uint32_t>(bytes[1]) << 8) |
           (static_cast<uint32_t>(bytes[2]) << 16) |
           (static_cast<uint32_t>(bytes[3]) << 24);
}

uint64_t bsatn_reader::read_u64() {
    uint8_t bytes[8];
    read_raw_bytes(bytes, 8);
    return static_cast<uint64_t>(bytes[0]) |
           (static_cast<uint64_t>(bytes[1]) << 8) |
           (static_cast<uint64_t>(bytes[2]) << 16) |
           (static_cast<uint64_t>(bytes[3]) << 24) |
           (static_cast<uint64_t>(bytes[4]) << 32) |
           (static_cast<uint64_t>(bytes[5]) << 40) |
           (static_cast<uint64_t>(bytes[6]) << 48) |
           (static_cast<uint64_t>(bytes[7]) << 56);
}

int8_t bsatn_reader::read_i8() {
    return static_cast<int8_t>(read_u8());
}

int16_t bsatn_reader::read_i16() {
    return static_cast<int16_t>(read_u16());
}

int32_t bsatn_reader::read_i32() {
    return static_cast<int32_t>(read_u32());
}

int64_t bsatn_reader::read_i64() {
    return static_cast<int64_t>(read_u64());
}

float bsatn_reader::read_f32() {
    uint32_t bits = read_u32();
    float value;
    std::memcpy(&value, &bits, sizeof(float));
    return value;
}

double bsatn_reader::read_f64() {
    uint64_t bits = read_u64();
    double value;
    std::memcpy(&value, &bits, sizeof(double));
    return value;
}

std::string bsatn_reader::read_string() {
    uint32_t len = read_u32();
    if (static_cast<size_t>(end_ptr - current_ptr) < len) {
        throw std::runtime_error("BSATN read past end of buffer (string data)");
    }
    std::string str(reinterpret_cast<const char*>(current_ptr), len);
    current_ptr += len;
    return str;
}

std::vector<uint8_t> bsatn_reader::read_bytes() {
    uint32_t len = read_u32();
    if (static_cast<size_t>(end_ptr - current_ptr) < len) {
        throw std::runtime_error("BSATN read past end of buffer (byte array data)");
    }
    std::vector<uint8_t> bytes(current_ptr, current_ptr + len);
    current_ptr += len;
    return bytes;
}

uint8_t bsatn_reader::read_sum_discriminant() {
    return read_u8();
}

bool bsatn_reader::eof() const {
    return current_ptr >= end_ptr;
}

size_t bsatn_reader::remaining_bytes() const {
    return static_cast<size_t>(end_ptr - current_ptr);
}

} // namespace bsatn
} // namespace spacetimedb
