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
#define STDB_TEST_TRANSACTION_FINISHED 5u
#define STDB_TEST_TRANSACTION_ACTIVE 6u
#define STDB_TEST_DATASTORE_CLOSED 7u
#define STDB_TEST_UNIQUE_VIOLATION 8u
#define STDB_TEST_AUTO_INC_OVERFLOW 9u

typedef struct StdbTestDatastore StdbTestDatastore;
typedef struct StdbTestTransaction StdbTestTransaction;
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
 * owner thread so transactions can be released from any caller thread
 * (including a managed finalizer). Destroy waits for owner-thread cleanup.
 * A non-null handle is consumed even on failure and must never be reused.
 * Null is a no-op. Invalid/stale pointers are caller errors, not checked handles.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_datastore_destroy(StdbTestDatastore *datastore);

/* One pending transaction per datastore. Nested begin returns TRANSACTION_ACTIVE
 * rather than waiting for the existing transaction's lock. All table operations
 * below use this explicit transaction; none open another transaction.
 * All output slots and error are required and initialized on failure as well as
 * success. Scalar outputs are zeroed and buffer/handle outputs are null/empty.
 * Table names are UTF-8, not NUL terminated; IDs belong to this datastore.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_transaction_begin(
    const StdbTestDatastore *datastore, StdbTestTransaction **out, StdbTestBuffer *error);

/* Commit/rollback finish the transaction but do not free its handle, even if the
 * database reports an error. Further operations return TRANSACTION_FINISHED.
 * Argument errors do not finish it. Table-operation errors leave it pending;
 * callers should roll back to discard any partial changes. An internal panic
 * stops the worker and rolls back pending work; later calls report DATASTORE_CLOSED.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_transaction_commit(
    const StdbTestTransaction *transaction, StdbTestBuffer *error);
uint32_t STDB_TEST_CALL stdb_test_v1_transaction_rollback(
    const StdbTestTransaction *transaction, StdbTestBuffer *error);

/* Consumes the handle; rolls back if still pending. Null is a no-op. May run on
 * any caller thread, but never concurrently with another call using this handle.
 * Datastore destruction rolls back pending work even if transaction handles
 * survive it. Those handles report DATASTORE_CLOSED and remain safe to destroy.
 * Destroying a finished handle never affects a newer transaction.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_transaction_destroy(StdbTestTransaction *transaction);
uint32_t STDB_TEST_CALL stdb_test_v1_table_id(
    const StdbTestTransaction *transaction, const uint8_t *name, uint64_t name_len,
    uint32_t *out, StdbTestBuffer *error);
uint32_t STDB_TEST_CALL stdb_test_v1_table_count(
    const StdbTestTransaction *transaction, uint32_t table_id,
    uint64_t *out, StdbTestBuffer *error);

/* Scan returns a snapshot encoded as BSATN array<array<u8>>, each inner array
 * containing one BSATN row. Row order is unspecified. Free via buffer_free.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_table_scan(
    const StdbTestTransaction *transaction, uint32_t table_id,
    StdbTestBuffer *out, StdbTestBuffer *error);

/* Insert borrows a BSATN row and returns the BSATN product of generated column
 * values, in the engine's generated-column projection order. Apply these values
 * to the caller's row; free the returned buffer even if writeback fails. A row
 * with no generated values returns an empty product (zero bytes).
 */
uint32_t STDB_TEST_CALL stdb_test_v1_table_insert(
    const StdbTestTransaction *transaction, uint32_t table_id,
    const uint8_t *row, uint64_t row_len, StdbTestBuffer *out, StdbTestBuffer *error);

/* Index names are UTF-8; IDs are local to the datastore. A point key is encoded
 * as the index key type: a scalar for a single column, a product for multiple
 * columns. Scan buffers have the same array<array<u8>> encoding as table_scan.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_index_id(
    const StdbTestTransaction *transaction, const uint8_t *name, uint64_t name_len,
    uint32_t *out, StdbTestBuffer *error);
uint32_t STDB_TEST_CALL stdb_test_v1_index_scan_point(
    const StdbTestTransaction *transaction, uint32_t index_id,
    const uint8_t *point, uint64_t point_len, StdbTestBuffer *out, StdbTestBuffer *error);
uint32_t STDB_TEST_CALL stdb_test_v1_index_delete_point(
    const StdbTestTransaction *transaction, uint32_t index_id,
    const uint8_t *point, uint64_t point_len, uint32_t *out, StdbTestBuffer *error);

/* A range fixes the first prefix_elems index columns to a BSATN product prefix.
 * start/end are BSATN Bound values on the next index column, using the engine's
 * encoding: tag 0 = Included(value), 1 = Excluded(value), 2 = Unbounded.
 * Any later columns are unrestricted. The bound value has that column's type.
 * An empty prefix uses prefix_elems = 0 and
 * zero bytes. Bounds are required, including the tag for Unbounded.
 * Delete operations return the number of rows deleted.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_index_scan_range(
    const StdbTestTransaction *transaction, uint32_t index_id,
    const uint8_t *prefix, uint64_t prefix_len, uint16_t prefix_elems,
    const uint8_t *start, uint64_t start_len, const uint8_t *end, uint64_t end_len,
    StdbTestBuffer *out, StdbTestBuffer *error);
uint32_t STDB_TEST_CALL stdb_test_v1_index_delete_range(
    const StdbTestTransaction *transaction, uint32_t index_id,
    const uint8_t *prefix, uint64_t prefix_len, uint16_t prefix_elems,
    const uint8_t *start, uint64_t start_len, const uint8_t *end, uint64_t end_len,
    uint32_t *out, StdbTestBuffer *error);

/* Update locates the old row via the unique index using key fields from the
 * supplied full row. It returns generated columns exactly as table_insert does.
 * Insert/update report UNIQUE_VIOLATION and AUTO_INC_OVERFLOW as structured
 * statuses; diagnostic text is not a machine-readable error discriminator.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_table_update(
    const StdbTestTransaction *transaction, uint32_t table_id, uint32_t index_id,
    const uint8_t *row, uint64_t row_len, StdbTestBuffer *out, StdbTestBuffer *error);

/* Delete matches the entire row, not merely its primary key; returns 0 or 1.
 * Clear returns the row count removed, preserving schema and sequence state.
 * Both participate in the transaction and can be rolled back.
 * Event rows remain visible in their transaction but are not retained at commit;
 * the engine handles this for both automatic and explicit transactions.
 */
uint32_t STDB_TEST_CALL stdb_test_v1_table_delete_row(
    const StdbTestTransaction *transaction, uint32_t table_id,
    const uint8_t *row, uint64_t row_len, uint32_t *out, StdbTestBuffer *error);
uint32_t STDB_TEST_CALL stdb_test_v1_table_clear(
    const StdbTestTransaction *transaction, uint32_t table_id,
    uint64_t *out, StdbTestBuffer *error);

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
