#include <spacetimedb/sdk/spacetimedb_sdk_table_registry.h>
#include <map> // Ensure map is included for the definition
// #include <mutex> // Not strictly needed for typical WASM single-thread, but good for general C++ static init

namespace spacetimedb {
namespace sdk {
namespace registry {

// Definition of the global table registry
// Using a function static to ensure initialization order.
// In C++11 and later, static local variable initialization is thread-safe.
static std::map<std::string, TableMetadata>& actual_global_table_registry_instance() {
    static std::map<std::string, TableMetadata> registry_map;
    return registry_map;
}

// Provide access to the global registry
std::map<std::string, TableMetadata>& get_global_table_registry() {
    return actual_global_table_registry_instance();
}

const TableMetadata* get_table_metadata_by_cpp_type_name(const std::string& cpp_type_name_mangled) {
    auto& registry = get_global_table_registry();
    auto it = registry.find(cpp_type_name_mangled);
    if (it != registry.end()) {
        return &it->second;
    }
    return nullptr;
}

const TableMetadata* get_table_metadata_by_db_name(const std::string& db_table_name) {
    auto& registry = get_global_table_registry();
    // This requires iterating through the map values.
    // If this lookup is frequent, a second map keyed by db_table_name might be beneficial
    // or the registry could be structured differently (e.g. boost.bimap or two maps).
    for (const auto& pair : registry) {
        if (pair.second.table_name_in_db == db_table_name) {
            return &pair.second;
        }
    }
    return nullptr;
}

uint32_t get_pk_column_index_by_cpp_type_name(const std::string& cpp_type_name_mangled) {
    const TableMetadata* metadata = get_table_metadata_by_cpp_type_name(cpp_type_name_mangled);
    if (metadata && !metadata->primary_key_field_name.empty()) {
        // This returns the stored index, which the macro sets to 0 if a PK name is given.
        return metadata->primary_key_column_index;
    }
    // Return a sentinel value if no PK info or table not found
    return static_cast<uint32_t>(-1); // Indicates no PK or table not found
}

} // namespace registry
} // namespace sdk
} // namespace spacetimedb
