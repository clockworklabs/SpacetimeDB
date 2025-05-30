#ifndef SPACETIMEDB_SDK_TABLE_REGISTRY_H
#define SPACETIMEDB_SDK_TABLE_REGISTRY_H

#include <string>
#include <vector>
#include <map>
#include <stdexcept> // For potential errors
#include <typeinfo>  // For typeid
#include <algorithm> // For std::find_if in get_table_metadata_by_db_name
#include <cstdint>   // For uint32_t

// Forward declaration if TableMetadata needs to know about BsatnSerializable, though not directly.
// class BsatnSerializable;

namespace spacetimedb {
namespace sdk {
namespace registry { // Encapsulate registry specific components

struct TableMetadata {
    std::string table_name_in_db;
    std::string cpp_type_name; // Result of typeid(T).name()
    std::string primary_key_field_name; // Name of the PK field in the C++ struct
    uint32_t primary_key_column_index; // Assumed to be 0 if pk field name is provided

    TableMetadata(std::string db_name = "", std::string cpp_name = "", std::string pk_name = "", uint32_t pk_idx = static_cast<uint32_t>(-1))
        : table_name_in_db(std::move(db_name)),
          cpp_type_name(std::move(cpp_name)),
          primary_key_field_name(std::move(pk_name)),
          primary_key_column_index(pk_idx) {}
};

// Global registry instance. Keyed by C++ type name (mangled).
// Definition will be in the .cpp file.
extern std::map<std::string, TableMetadata>& get_global_table_registry();

// Accessor functions
const TableMetadata* get_table_metadata_by_cpp_type_name(const std::string& cpp_type_name_mangled);
const TableMetadata* get_table_metadata_by_db_name(const std::string& db_table_name);

// Helper to get PK column index. Returns static_cast<uint32_t>(-1) if not found or no PK registered.
uint32_t get_pk_column_index_by_cpp_type_name(const std::string& cpp_type_name_mangled);


// Registration helper struct (used by the macro)
// This struct's constructor will do the actual registration.
struct TableRegistrar {
    TableRegistrar(const char* cpp_type_name_mangled, // typeid().name() returns const char*
                   const std::string& table_name_in_db_str,
                   const std::string& pk_field_name_str) {
        TableMetadata metadata;
        metadata.cpp_type_name = cpp_type_name_mangled;
        metadata.table_name_in_db = table_name_in_db_str;
        metadata.primary_key_field_name = pk_field_name_str;

        if (!pk_field_name_str.empty()) {
            metadata.primary_key_column_index = 0; // Assumed first field if PK is named
        } else {
            // No primary key specified, use a sentinel for no PK
            metadata.primary_key_column_index = static_cast<uint32_t>(-1);
        }

        auto& registry = get_global_table_registry();
        // Using insert to avoid overwriting if already registered from another TU,
        // though static init in anonymous namespace should make registrar_instance unique per TU.
        // If multiple TUs register the same CppStructType, this will only insert the first one.
        registry.insert({std::string(cpp_type_name_mangled), metadata});
    }
};

} // namespace registry

// User-facing macro
// If PrimaryKeyFieldAsString is empty, it means no PK or PK is not the first field / not named here.
#define SPACETIMEDB_REGISTER_TABLE(CppStructType, TableNameInDbString, PrimaryKeyFieldAsString) \
    namespace { /* Anonymous namespace to ensure unique registrar_instance per TU */ \
        static spacetimedb::sdk::registry::TableRegistrar \
            registrar_instance_##CppStructType( \
                typeid(CppStructType).name(), \
                TableNameInDbString, \
                PrimaryKeyFieldAsString \
            ); \
    }


// Inline accessor template for convenience, using typeid directly
template<typename T>
const registry::TableMetadata* get_table_metadata() {
    return registry::get_table_metadata_by_cpp_type_name(typeid(T).name());
}

template<typename T>
uint32_t get_pk_column_index() {
     return registry::get_pk_column_index_by_cpp_type_name(typeid(T).name());
}


} // namespace sdk
} // namespace spacetimedb

#endif // SPACETIMEDB_SDK_TABLE_REGISTRY_H
