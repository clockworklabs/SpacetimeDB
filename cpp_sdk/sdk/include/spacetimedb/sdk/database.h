#ifndef SPACETIMEDB_SDK_DATABASE_H
#define SPACETIMEDB_SDK_DATABASE_H

#include <string>
#include <stdexcept> // For std::runtime_error
#include <spacetimedb/sdk/table.h> // For Table<T>
#include <spacetimedb/abi/spacetimedb_abi.h> // For ABI function calls
#include <spacetimedb/bsatn/bsatn.h> // For BsatnSerializable concept (implicitly via Table<T>)

namespace spacetimedb {
namespace sdk {

class Database {
public:
    Database();

    template<typename T>
    Table<T> get_table(const std::string& table_name) {
        // ABI: uint16_t _get_table_id(const uint8_t *name_ptr, size_t name_len, uint32_t *out_table_id_ptr)

        uint32_t table_id = 0;
        uint16_t error_code = _get_table_id(
            reinterpret_cast<const uint8_t*>(table_name.c_str()),
            table_name.length(),
            &table_id
        );

        if (error_code != 0) {
            throw std::runtime_error("Database::get_table: _get_table_id ABI call failed for table '" +
                                     table_name + "' with error code " + std::to_string(error_code));
        }

        // It's possible that an error_code of 0 still means "not found" if table_id is a sentinel like 0.
        // This depends on the ABI contract for _get_table_id. Assuming 0 is an invalid ID if no error.
        if (table_id == 0 && error_code == 0) {
            throw std::runtime_error("Table not found: " + table_name + " (table_id resolved to 0 without ABI error)");
        }

        return Table<T>(table_id);
    }
};

} // namespace sdk
} // namespace spacetimedb

#endif // SPACETIMEDB_SDK_DATABASE_H
