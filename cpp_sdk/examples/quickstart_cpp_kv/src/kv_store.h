#ifndef KV_STORE_H
#define KV_STORE_H

// SDK Headers - Assuming these are findable via include paths set up by CMake
// e.g., target_include_directories( ... PUBLIC ${SPACETIMEDB_SDK_INCLUDE_DIR})
// where SPACETIMEDB_SDK_INCLUDE_DIR points to the 'sdk/include' directory.
// The user would then #include "spacetimedb/sdk/types.h" etc. if headers are organized under spacetimedb/sdk
// For this regeneration, I'll assume the SDK headers are structured to be included like this:
#include <spacetimedb/sdk/spacetimedb_sdk_types.h>
#include <spacetimedb/bsatn/bsatn.h> // Assuming bsatn.h is under spacetimedb/bsatn path
#include <spacetimedb/sdk/spacetimedb_sdk_table_registry.h>
#include <spacetimedb/sdk/reducer_context.h>

#include <string>
#include <vector> // Though not directly used in KeyValue, often useful

namespace spacetimedb_quickstart {

// Log levels for direct _console_log usage
// These should ideally match any enum or constants defined by the host or ABI for clarity
const uint8_t LOG_LEVEL_FATAL = 0;
const uint8_t LOG_LEVEL_ERROR = 1;
const uint8_t LOG_LEVEL_WARN = 2;
const uint8_t LOG_LEVEL_INFO = 3;
const uint8_t LOG_LEVEL_DEBUG = 4;
const uint8_t LOG_LEVEL_TRACE = 5;


struct KeyValue : public spacetimedb::sdk::bsatn::BsatnSerializable {
    std::string key_str;   // Primary Key
    std::string value_str;

    // Default constructor
    KeyValue() = default;

    // Constructor for convenience
    KeyValue(std::string k, std::string v) : key_str(std::move(k)), value_str(std::move(v)) {}

    // BsatnSerializable interface
    void bsatn_serialize(spacetimedb::sdk::bsatn::bsatn_writer& writer) const override;
    void bsatn_deserialize(spacetimedb::sdk::bsatn::bsatn_reader& reader) override;

    // Optional: Comparison operator for potential use in tests or other logic
    bool operator==(const KeyValue& other) const {
        return key_str == other.key_str && value_str == other.value_str;
    }
};

// Reducer function declarations
void kv_put(spacetimedb::sdk::ReducerContext& ctx, const std::string& key, const std::string& value);
void kv_get(spacetimedb::sdk::ReducerContext& ctx, const std::string& key);
void kv_del(spacetimedb::sdk::ReducerContext& ctx, const std::string& key);

} // namespace spacetimedb_quickstart

#endif // KV_STORE_H
