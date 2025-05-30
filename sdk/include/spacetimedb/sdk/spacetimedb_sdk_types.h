#ifndef SPACETIMEDB_SDK_TYPES_H
#define SPACETIMEDB_SDK_TYPES_H

#include <cstdint>
#include <vector>
#include <string>
#include <array>
#include <chrono> // For Timestamp::current()
#include <stdexcept> // For std::runtime_error in Identity deserialization

// Forward declarations from bsatn.h - needed for BsatnSerializable usage
namespace spacetimedb {
namespace bsatn {
class bsatn_writer;
class bsatn_reader;
class BsatnSerializable; // Ensure BsatnSerializable itself is forward-declared or included if it's a base
} // namespace bsatn
} // namespace spacetimedb


namespace spacetimedb {
namespace sdk {

const size_t IDENTITY_SIZE = 32;

class Identity {
public:
    Identity();
    explicit Identity(const std::array<uint8_t, IDENTITY_SIZE>& bytes);
    // static Identity from_hex_string(const std::string& hex_str); // Implementation can be added if needed

    const std::array<uint8_t, IDENTITY_SIZE>& get_bytes() const;
    std::string to_hex_string() const;

    bool operator==(const Identity& other) const;
    bool operator!=(const Identity& other) const;
    bool operator<(const Identity& other) const; // For std::map keys

    // BSATN Serialization methods (duck-typed, or inherit BsatnSerializable)
    void bsatn_serialize(bsatn::bsatn_writer& writer) const;
    void bsatn_deserialize(bsatn::bsatn_reader& reader);

private:
    std::array<uint8_t, IDENTITY_SIZE> value;
};

class Timestamp {
public:
    Timestamp();
    explicit Timestamp(uint64_t milliseconds_since_epoch);

    uint64_t as_milliseconds() const;

    static Timestamp current();

    bool operator==(const Timestamp& other) const;
    bool operator!=(const Timestamp& other) const;
    bool operator<(const Timestamp& other) const;
    bool operator<=(const Timestamp& other) const;
    bool operator>(const Timestamp& other) const;
    bool operator>=(const Timestamp& other) const;

    // BSATN Serialization methods
    void bsatn_serialize(bsatn::bsatn_writer& writer) const;
    void bsatn_deserialize(bsatn::bsatn_reader& reader);

private:
    uint64_t ms_since_epoch;
};

} // namespace sdk
} // namespace spacetimedb

#endif // SPACETIMEDB_SDK_TYPES_H
