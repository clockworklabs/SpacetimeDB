#ifndef SPACETIMEDB_TEST_DATASTORE_H
#define SPACETIMEDB_TEST_DATASTORE_H

#include <stdint.h>

#ifdef _WIN32
#define STDB_TEST_CALL __cdecl
#else
#define STDB_TEST_CALL
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define STDB_TEST_ABI_VERSION 1u
#define STDB_TEST_OK 0u
#define STDB_TEST_INVALID_ARGUMENT 1u
#define STDB_TEST_INVALID_SCHEMA 2u
#define STDB_TEST_DATABASE_ERROR 3u
#define STDB_TEST_INTERNAL_ERROR 4u

typedef struct StdbTestDatastore StdbTestDatastore;
typedef struct StdbTestBuffer {
    uint8_t *data;
    uint64_t len;
} StdbTestBuffer;

uint32_t STDB_TEST_CALL stdb_test_abi_version(void);

/* All pointers must refer to valid, aligned, nonoverlapping storage. Input bytes
 * are borrowed only for the duration of the call. Output slots must be empty;
 * create initializes both outputs even on failure. error is required and holds
 * an owned UTF-8 diagnostic (not NUL terminated) on failure, empty on success.
 * Free it with buffer_free; never free native allocations with the caller's allocator.
 * schema is one complete BSATN RawModuleDef, including its version discriminant.
 * Null schema is permitted only with length zero (which is an invalid schema).
 * Check the ABI version before calling versioned functions.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_datastore_create(
    const uint8_t *schema, uint64_t schema_len,
    StdbTestDatastore **out, StdbTestBuffer *error);

/* Calls using a handle must be serialized by the caller, including destruction.
 * Handles may move between caller threads. Native state lives on a dedicated
 * owner thread so transactions can eventually be released from any caller thread
 * (including a managed finalizer). Destroy waits for owner-thread cleanup.
 * A non-null handle is consumed even on failure and must never be reused.
 * Null is a no-op. Invalid/stale pointers are caller errors, not checked handles.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_datastore_destroy(StdbTestDatastore *datastore);

/* Free an unmodified returned buffer exactly once, then zero the supplied slot.
 * Null and an empty buffer are no-ops. Copies of a buffer do not share ownership.
 * Functions return status codes; Rust unwinding panics never cross the ABI.
 * Process aborts, allocation failure, and invalid caller memory are not recoverable.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_buffer_free(StdbTestBuffer *buffer);

#ifdef __cplusplus
}
#endif
#endif
