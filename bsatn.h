#ifndef BSATN_H
#define BSATN_H

#include <vector>
#include <string>
#include <cstdint>
#include <stdexcept>     // For std::runtime_error
#include <algorithm>     // For std::reverse (potentially, if not doing direct byte manipulation)
#include <type_traits>   // For std::is_integral, std::is_floating_point, std::is_base_of_v, std::is_same_v
#include <cstring>       // For memcpy

// Forward declaration for user-defined types
class BsatnSerializable;

namespace spacetimedb {
namespace bsatn {

class bsatn_writer {
public:
    bsatn_writer();

    void write_bool(bool value);
    void write_u8(uint8_t value);
    void write_u16(uint16_t value);
    void write_u32(uint32_t value);
    void write_u64(uint64_t value);
    void write_i8(int8_t value);
    void write_i16(int16_t value);
    void write_i32(int32_t value);
    void write_i64(int64_t value);
    void write_f32(float value);
    void write_f64(double value);
    void write_string(const std::string& str);
    void write_bytes(const std::vector<uint8_t>& bytes);

    template<typename T>
    void write_object(const T& obj);

    template<typename T>
    void write_array(const std::vector<T>& vec);

    void write_sum_discriminant(uint8_t discriminant);

    const std::vector<uint8_t>& get_buffer() const;
    std::vector<uint8_t>&& move_buffer();

private:
    void write_raw_bytes(const void* data, size_t len);
    std::vector<uint8_t> buffer;
};

class bsatn_reader {
public:
    bsatn_reader(const uint8_t* data, size_t len);
    bsatn_reader(const std::vector<uint8_t>& data);

    bool read_bool();
    uint8_t read_u8();
    uint16_t read_u16();
    uint32_t read_u32();
    uint64_t read_u64();
    int8_t read_i8();
    int16_t read_i16();
    int32_t read_i32();
    int64_t read_i64();
    float read_f32();
    double read_f64();
    std::string read_string();
    std::vector<uint8_t> read_bytes();

    template<typename T>
    void read_object(T& obj);

    template<typename T>
    std::vector<T> read_array();
    
    uint8_t read_sum_discriminant();

    bool eof() const;
    size_t remaining_bytes() const;

private:
    void read_raw_bytes(void* data, size_t len);
    const uint8_t* current_ptr;
    const uint8_t* end_ptr;
};

class BsatnSerializable {
public:
    virtual ~BsatnSerializable() = default;
    virtual void bsatn_serialize(bsatn_writer& writer) const = 0;
    virtual void bsatn_deserialize(bsatn_reader& reader) = 0;
};

// Template implementations for bsatn_writer
template<typename T>
void bsatn_writer::write_object(const T& obj) {
    obj.bsatn_serialize(*this);
}

template<typename T>
void bsatn_writer::write_array(const std::vector<T>& vec) {
    write_u32(static_cast<uint32_t>(vec.size()));
    for (const auto& item : vec) {
        if constexpr (std::is_same_v<T, bool>) write_bool(item);
        else if constexpr (std::is_same_v<T, uint8_t>) write_u8(item);
        else if constexpr (std::is_same_v<T, uint16_t>) write_u16(item);
        else if constexpr (std::is_same_v<T, uint32_t>) write_u32(item);
        else if constexpr (std::is_same_v<T, uint64_t>) write_u64(item);
        else if constexpr (std::is_same_v<T, int8_t>) write_i8(item);
        else if constexpr (std::is_same_v<T, int16_t>) write_i16(item);
        else if constexpr (std::is_same_v<T, int32_t>) write_i32(item);
        else if constexpr (std::is_same_v<T, int64_t>) write_i64(item);
        else if constexpr (std::is_same_v<T, float>) write_f32(item);
        else if constexpr (std::is_same_v<T, double>) write_f64(item);
        else if constexpr (std::is_same_v<T, std::string>) write_string(item);
        else if constexpr (std::is_same_v<T, std::vector<uint8_t>>) write_bytes(item); // For vector of bytes
        else if constexpr (std::is_base_of_v<BsatnSerializable, T> || requires(const T& t, bsatn_writer& w) { t.bsatn_serialize(w); }) {
            write_object(item);
        } else {
            // This static_assert will fire if T is not one of the above or doesn't have bsatn_serialize
            static_assert(std::is_void_v<T>, "Unsupported type in write_array. Type must be a primitive, std::string, std::vector<uint8_t>, or implement bsatn_serialize.");
        }
    }
}

// Template implementations for bsatn_reader
template<typename T>
void bsatn_reader::read_object(T& obj) {
    obj.bsatn_deserialize(*this);
}

template<typename T>
std::vector<T> bsatn_reader::read_array() {
    uint32_t size = read_u32();
    std::vector<T> vec;
    vec.reserve(size);
    for (uint32_t i = 0; i < size; ++i) {
        if constexpr (std::is_same_v<T, bool>) vec.push_back(read_bool());
        else if constexpr (std::is_same_v<T, uint8_t>) vec.push_back(read_u8());
        else if constexpr (std::is_same_v<T, uint16_t>) vec.push_back(read_u16());
        else if constexpr (std::is_same_v<T, uint32_t>) vec.push_back(read_u32());
        else if constexpr (std::is_same_v<T, uint64_t>) vec.push_back(read_u64());
        else if constexpr (std::is_same_v<T, int8_t>) vec.push_back(read_i8());
        else if constexpr (std::is_same_v<T, int16_t>) vec.push_back(read_i16());
        else if constexpr (std::is_same_v<T, int32_t>) vec.push_back(read_i32());
        else if constexpr (std::is_same_v<T, int64_t>) vec.push_back(read_i64());
        else if constexpr (std::is_same_v<T, float>) vec.push_back(read_f32());
        else if constexpr (std::is_same_v<T, double>) vec.push_back(read_f64());
        else if constexpr (std::is_same_v<T, std::string>) vec.push_back(read_string());
        else if constexpr (std::is_same_v<T, std::vector<uint8_t>>) vec.push_back(read_bytes());
        else if constexpr (std::is_base_of_v<BsatnSerializable, T> || requires(T& t, bsatn_reader& r) { t.bsatn_deserialize(r); }) {
            T item;
            read_object(item);
            vec.push_back(std::move(item));
        } else {
            static_assert(std::is_void_v<T>, "Unsupported type in read_array. Type must be a primitive, std::string, std::vector<uint8_t>, or implement bsatn_deserialize.");
        }
    }
    return vec;
}

} // namespace bsatn
} // namespace spacetimedb

#endif // BSATN_H
