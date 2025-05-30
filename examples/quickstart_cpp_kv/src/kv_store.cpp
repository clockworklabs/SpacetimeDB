#include "kv_store.h" // Local header

// SDK Headers
#include <spacetimedb/sdk/spacetimedb_sdk_reducer.h>
#include <spacetimedb/sdk/database.h>
#include <spacetimedb/sdk/table.h>
#include <spacetimedb/abi/spacetimedb_abi.h> // For direct ABI calls like _console_log

// Standard Library
#include <string>
#include <vector>
#include <stdexcept> // For std::runtime_error (used by SDK components)
#include <cstring>   // For std::strlen in log_message_abi

// The _spacetimedb_sdk_init() function is defined in spacetimedb_sdk_reducer.h
// and will be exported. The host calls it to initialize the SDK, including the
// global database instance needed by the ReducerContext.

namespace spacetimedb_quickstart {

// KeyValue BSATN implementation
void KeyValue::bsatn_serialize(spacetimedb::sdk::bsatn::bsatn_writer& writer) const {
    writer.write_string(key_str);   // First field, assumed PK (index 0)
    writer.write_string(value_str); // Second field
}

void KeyValue::bsatn_deserialize(spacetimedb::sdk::bsatn::bsatn_reader& reader) {
    key_str = reader.read_string();
    value_str = reader.read_string();
}

// Register the KeyValue table with the SDK's global registry.
// "key_str" is declared as the primary key field.
// The SDK's table registration currently assumes if a PK field name is provided,
// its column index is 0 (i.e., it's the first field serialized).
SPACETIMEDB_REGISTER_TABLE(spacetimedb_quickstart::KeyValue, "kv_pairs", "key_str");

// Helper for logging from reducers via the raw ABI
static void log_message_abi(uint8_t level, const std::string& context_info, const std::string& message) {
    std::string full_message = "[" + context_info + "] " + message;
    // _console_log(uint8_t level, const uint8_t *target, size_t target_len,
    //              const uint8_t *filename, size_t filename_len, uint32_t line_number,
    //              const uint8_t *text, size_t text_len)
    _console_log(level,
                 nullptr, 0,  // target (e.g. module path) - omitting for simplicity
                 nullptr, 0,  // filename - omitting for simplicity
                 0,           // line_number - omitting for simplicity
                 reinterpret_cast<const uint8_t*>(full_message.c_str()),
                 full_message.length());
}

// Reducer Implementations

void kv_put(spacetimedb::sdk::ReducerContext& ctx, const std::string& key, const std::string& value) {
    std::string reducer_name = "kv_put";
    try {
        auto kv_table = ctx.db().get_table<KeyValue>("kv_pairs");

        // To simulate an "upsert", we first delete any existing entry with the same key, then insert.
        // This assumes `key_str` is the primary key and at column index 0.
        // The TableMetadata registry sets pk_column_index to 0 if "key_str" is the PK.
        uint32_t pk_col_idx = spacetimedb::sdk::get_pk_column_index<KeyValue>();
        if (pk_col_idx != 0) {
             // This case should ideally not be reached if PK registration is correct and PK is first field.
             log_message_abi(LOG_LEVEL_WARN, reducer_name, "Warning: PK column index for KeyValue is not 0 as expected. Actual: " + std::to_string(pk_col_idx));
             // Potentially throw or use a default if this is critical, for now proceed with found/defaulted index.
        }

        kv_table.delete_by_col_eq(pk_col_idx, key); // Delete if exists (idempotent)

        KeyValue row_to_insert(key, value);
        kv_table.insert(row_to_insert);
        // Note: `insert` is in-out for `row_to_insert` but for KeyValue, PK is `key_str`, which we provide.
        // If PK was auto-generated, `row_to_insert` would be updated here.

        std::string log_msg = "Successfully put K-V: (" + key + ": " + value + ")";
        log_message_abi(LOG_LEVEL_INFO, reducer_name, log_msg);

    } catch (const std::runtime_error& e) {
        std::string error_msg = "Error: " + std::string(e.what());
        log_message_abi(LOG_LEVEL_ERROR, reducer_name, error_msg);
        // The reducer macro will catch this exception and return an error code to the host.
        throw; // Re-throw to be caught by the reducer macro wrapper
    }
}

void kv_get(spacetimedb::sdk::ReducerContext& ctx, const std::string& key) {
    std::string reducer_name = "kv_get";
    try {
        auto kv_table = ctx.db().get_table<KeyValue>("kv_pairs");
        uint32_t pk_col_idx = spacetimedb::sdk::get_pk_column_index<KeyValue>();

        std::vector<KeyValue> rows = kv_table.find_by_col_eq(pk_col_idx, key);

        if (!rows.empty()) {
            // Since key_str is PK, there should be at most one row.
            const auto& row = rows[0];
            std::string log_msg = "Found Key: " + row.key_str + ", Value: " + row.value_str;
            log_message_abi(LOG_LEVEL_INFO, reducer_name, log_msg);
        } else {
            std::string log_msg = "Key not found: " + key;
            log_message_abi(LOG_LEVEL_INFO, reducer_name, log_msg);
        }
    } catch (const std::runtime_error& e) {
        std::string error_msg = "Error: " + std::string(e.what());
        log_message_abi(LOG_LEVEL_ERROR, reducer_name, error_msg);
        throw;
    }
}

void kv_del(spacetimedb::sdk::ReducerContext& ctx, const std::string& key) {
    std::string reducer_name = "kv_del";
    try {
        auto kv_table = ctx.db().get_table<KeyValue>("kv_pairs");
        uint32_t pk_col_idx = spacetimedb::sdk::get_pk_column_index<KeyValue>();

        uint32_t deleted_count = kv_table.delete_by_col_eq(pk_col_idx, key);

        if (deleted_count > 0) {
            std::string log_msg = "Successfully deleted " + std::to_string(deleted_count) + " item(s) for key: " + key;
            log_message_abi(LOG_LEVEL_INFO, reducer_name, log_msg);
        } else {
            std::string log_msg = "No items found to delete for key: " + key;
            log_message_abi(LOG_LEVEL_INFO, reducer_name, log_msg);
        }
    } catch (const std::runtime_error& e) {
        std::string error_msg = "Error: " + std::string(e.what());
        log_message_abi(LOG_LEVEL_ERROR, reducer_name, error_msg);
        throw;
    }
}

// Register Reducers with the SDK.
// The types listed (e.g., const std::string&) must match the C++ function signature after ReducerContext.
// The actual exported WASM function name will be "kv_put", "kv_get", "kv_del".
SPACETIMEDB_REDUCER(spacetimedb_quickstart::kv_put, const std::string&, const std::string&);
SPACETIMEDB_REDUCER(spacetimedb_quickstart::kv_get, const std::string&);
SPACETIMEDB_REDUCER(spacetimedb_quickstart::kv_del, const std::string&);

} // namespace spacetimedb_quickstart
