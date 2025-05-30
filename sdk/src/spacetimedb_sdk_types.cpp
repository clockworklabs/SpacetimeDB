#include <spacetimedb/sdk/spacetimedb_sdk_types.h>
#include <spacetimedb/bsatn/bsatn.h> // For bsatn_writer, bsatn_reader
#include <iomanip>  // For std::setw, std::setfill with to_hex_string
#include <sstream>  // For std::ostringstream with to_hex_string
#include <algorithm> // for std::copy
#include <stdexcept> // For std::runtime_error

namespace spacetimedb {
namespace sdk {

// Identity Implementation
Identity::Identity() {
    value.fill(0);
}

Identity::Identity(const std::array<uint8_t, IDENTITY_SIZE>& bytes) : value(bytes) {}

const std::array<uint8_t, IDENTITY_SIZE>& Identity::get_bytes() const {
    return value;
}

std::string Identity::to_hex_string() const {
    std::ostringstream oss;
    oss << std::hex << std::setfill('0');
    for (uint8_t byte : value) {
        oss << std::setw(2) << static_cast<int>(byte);
    }
    return oss.str();
}

bool Identity::operator==(const Identity& other) const {
    return value == other.value;
}

bool Identity::operator!=(const Identity& other) const {
    return value != other.value;
}

bool Identity::operator<(const Identity& other) const {
    // Lexicographical comparison for std::map ordering
    return std::lexicographical_compare(value.begin(), value.end(),
                                        other.value.begin(), other.value.end());
}

void Identity::bsatn_serialize(bsatn::bsatn_writer& writer) const {
    // Serialize as a length-prefixed byte array
    std::vector<uint8_t> bytes_vec(value.begin(), value.end());
    writer.write_bytes(bytes_vec);
}

void Identity::bsatn_deserialize(bsatn::bsatn_reader& reader) {
    std::vector<uint8_t> bytes_vec = reader.read_bytes();
    if (bytes_vec.size() != IDENTITY_SIZE) {
        throw std::runtime_error("BSATN deserialization error: Identity size mismatch. Expected " +
                                 std::to_string(IDENTITY_SIZE) + ", got " + std::to_string(bytes_vec.size()));
    }
    std::copy(bytes_vec.begin(), bytes_vec.end(), value.begin());
}

// Timestamp Implementation
Timestamp::Timestamp() : ms_since_epoch(0) {}

Timestamp::Timestamp(uint64_t milliseconds_since_epoch) : ms_since_epoch(milliseconds_since_epoch) {}

uint64_t Timestamp::as_milliseconds() const {
    return ms_since_epoch;
}

Timestamp Timestamp::current() {
    auto now = std::chrono::system_clock::now();
    auto duration = now.time_since_epoch();
    return Timestamp(static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::milliseconds>(duration).count()));
}

bool Timestamp::operator==(const Timestamp& other) const {
    return ms_since_epoch == other.ms_since_epoch;
}

bool Timestamp::operator!=(const Timestamp& other) const {
    return ms_since_epoch != other.ms_since_epoch;
}

bool Timestamp::operator<(const Timestamp& other) const {
    return ms_since_epoch < other.ms_since_epoch;
}

bool Timestamp::operator<=(const Timestamp& other) const {
    return ms_since_epoch <= other.ms_since_epoch;
}

bool Timestamp::operator>(const Timestamp& other) const {
    return ms_since_epoch > other.ms_since_epoch;
}

bool Timestamp::operator>=(const Timestamp& other) const {
    return ms_since_epoch >= other.ms_since_epoch;
}

void Timestamp::bsatn_serialize(bsatn::bsatn_writer& writer) const {
    writer.write_u64(ms_since_epoch);
}

void Timestamp::bsatn_deserialize(bsatn::bsatn_reader& reader) {
    ms_since_epoch = reader.read_u64();
}

} // namespace sdk
} // namespace spacetimedb
