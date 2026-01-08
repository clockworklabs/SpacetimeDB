#ifndef SPACETIMEDB_BSATN_TYPES_IMPL_H
#define SPACETIMEDB_BSATN_TYPES_IMPL_H

// This file contains the implementation of BSATN serialization methods
// for SpacetimeDB types. It must be included after reader.h and writer.h
// to avoid circular dependencies.

#include "types.h"
#include "reader.h"
#include "writer.h"
#include <sstream>    // For std::ostringstream
#include <iomanip>    // For std::hex, std::setfill, std::setw

namespace SpacetimeDb {

// Identity method implementations
inline Identity::Identity() {
    value.fill(0);
}

inline Identity::Identity(const std::array<uint8_t, IDENTITY_SIZE>& bytes) : value(bytes) {
}

inline const std::array<uint8_t, IDENTITY_SIZE>& Identity::get_bytes() const {
    return value;
}

inline std::string Identity::to_hex_string() const {
    std::ostringstream oss;
    oss << std::hex << std::setfill('0');
    
    for (size_t i = 0; i < IDENTITY_SIZE; ++i) {
        oss << std::setw(2) << static_cast<unsigned int>(value[i]);
    }
    
    return oss.str();
}

inline bool Identity::operator==(const Identity& other) const {
    return value == other.value;
}

inline bool Identity::operator!=(const Identity& other) const {
    return !(*this == other);
}

inline bool Identity::operator<(const Identity& other) const {
    return value < other.value;
}

// Identity BSATN implementation
inline void Identity::bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const {
    // Write raw bytes without length prefix for fixed-size Identity
    for (size_t i = 0; i < this->value.size(); ++i) {
        writer.write_u8(this->value[i]);
    }
}

inline void Identity::bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader) {
    std::vector<uint8_t> bytes = reader.read_fixed_bytes(IDENTITY_SIZE);
    if (bytes.size() == IDENTITY_SIZE) {
        std::copy(bytes.begin(), bytes.end(), this->value.data());
    } else {
        std::abort();
    }
}

// ConnectionId BSATN implementation
inline void ConnectionId::bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const {
    writer.write_u128_le(this->id);
}

inline void ConnectionId::bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader) {
    this->id = reader.read_u128_le();
}

// u256 BSATN implementation
inline void u256::bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const {
    writer.write_u256_le(*this);
}

inline void u256::bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader) {
    std::vector<uint8_t> bytes = reader.read_fixed_bytes(sizeof(this->data));
    if (bytes.size() == sizeof(this->data)) {
        std::copy(bytes.begin(), bytes.end(), this->data.data());
    } else {
        std::abort();
    }
}

// i256 BSATN implementation
inline void i256::bsatn_serialize(::SpacetimeDb::bsatn::Writer& writer) const {
    writer.write_i256_le(*this);
}

inline void i256::bsatn_deserialize(::SpacetimeDb::bsatn::Reader& reader) {
    std::vector<uint8_t> bytes = reader.read_fixed_bytes(sizeof(this->data));
    if (bytes.size() == sizeof(this->data)) {
        std::copy(bytes.begin(), bytes.end(), this->data.data());
    } else {
        std::abort();
    }
}

// ScheduleAt BSATN implementation is in schedule_at_impl.h

// Note: u128 and i128 have their own serialize/deserialize static methods in types.h,
// not bsatn_serialize/deserialize member functions

} // namespace SpacetimeDb

#endif // SPACETIMEDB_BSATN_TYPES_IMPL_H