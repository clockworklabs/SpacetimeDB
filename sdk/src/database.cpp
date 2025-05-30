#include <spacetimedb/sdk/database.h>

// spacetimedb_abi.h is included via database.h -> table.h -> spacetimedb/abi/spacetimedb_abi.h
// If not, it should be included here for any ABI calls made directly by Database methods,
// though get_table<T> is templated and in the header.

namespace spacetimedb {
namespace sdk {

Database::Database() {
    // Constructor for Database.
    // If the Database class needed to initialize any state or acquire resources
    // via ABI calls at construction time, it would happen here.
    // For example, it might register itself with a host-side service
    // or pre-fetch some schema information if the ABI supported it.

    // For now, it's empty as get_table handles its own ABI interaction (conceptually).
    // If there were non-template methods in Database that made ABI calls,
    // those implementations would go here.
}

// Template methods like get_table<T>() are fully defined in database.h

// Other non-template Database methods would be implemented here.
// For example, if there were a non-template version of a query function:
// int Database::execute_raw_query(const std::string& query_str) {
//     // ... implementation using ABI calls ...
//     return 0; // status code
// }

} // namespace sdk
} // namespace spacetimedb
