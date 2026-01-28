#include "spacetimedb/abi/FFI.h"
#include "spacetimedb/internal/autogen/SumType.g.h"  // Complete SumType definition before Module.h
#include "spacetimedb/internal/Module.h"  // Use new Module API
#include "spacetimedb/bsatn/timestamp.h"     // For Timestamp

#include <vector>
#include <cstddef> // For size_t
#include <string>  // For std::string in error handling
#include <iostream> // For temporary error logging if needed

// Export definitions - these implement the declarations from abi.h

extern "C" {

    STDB_EXPORT(__describe_module__)
    void __describe_module__(SpacetimeDB::BytesSink description) {
        // Use the new Module API directly with opaque type
        SpacetimeDB::Internal::Module::__describe_module__(description);
    }

    STDB_EXPORT(__call_reducer__)
    int16_t __call_reducer__(
        uint32_t reducer_id,
        uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
        uint64_t conn_id_0, uint64_t conn_id_1,
        uint64_t timestamp_us,
        SpacetimeDB::BytesSource args,
        SpacetimeDB::BytesSink error
    ) {
        // Create timestamp - already in microseconds
        SpacetimeDB::Timestamp ts(timestamp_us);
        
        // Call Module's implementation with opaque types
        auto result = SpacetimeDB::Internal::Module::__call_reducer__(
            reducer_id,
            sender_0, sender_1, sender_2, sender_3,
            conn_id_0, conn_id_1,
            ts,
            args,
            error
        );
        
        // Convert Status to int16_t
        if (result == SpacetimeDB::StatusCode::OK) {
            return 0;
        } else if (result == SpacetimeDB::StatusCode::NO_SUCH_REDUCER) {
            return -1;
        } else if (result == SpacetimeDB::StatusCode::HOST_CALL_FAILURE) {
            return 1;
        } else {
            return -4;
        }
    }

    STDB_EXPORT(__call_view__)
    int16_t __call_view__(
        uint32_t view_id,
        uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
        SpacetimeDB::BytesSource args,
        SpacetimeDB::BytesSink result
    ) {
        return SpacetimeDB::Internal::Module::__call_view__(
            view_id,
            sender_0, sender_1, sender_2, sender_3,
            args,
            result
        );
    }

    STDB_EXPORT(__call_view_anon__)
    int16_t __call_view_anon__(
        uint32_t view_id,
        SpacetimeDB::BytesSource args,
        SpacetimeDB::BytesSink result
    ) {
        return SpacetimeDB::Internal::Module::__call_view_anon__(
            view_id,
            args,
            result
        );
    }

    STDB_EXPORT(__call_procedure__)
    int16_t __call_procedure__(
        uint32_t id,
        uint64_t sender_0, uint64_t sender_1, uint64_t sender_2, uint64_t sender_3,
        uint64_t conn_id_0, uint64_t conn_id_1,
        uint64_t timestamp_microseconds,
        SpacetimeDB::BytesSource args_source,
        SpacetimeDB::BytesSink result_sink
    ) {
        return SpacetimeDB::Internal::Module::__call_procedure__(
            id,
            sender_0, sender_1, sender_2, sender_3,
            conn_id_0, conn_id_1,
            timestamp_microseconds,
            args_source,
            result_sink
        );
    }

} // extern "C"
