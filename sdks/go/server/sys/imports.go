//go:build wasip1

package sys

// spacetime_10.0 host functions
//
// NOTE: //go:wasmimport only supports int32/uint32/int64/uint64/float32/float64/unsafe.Pointer.
// The WASM ABI uses i32 for all sub-32-bit types, so we use uint32/int32 here
// and cast to the appropriate narrower types in the Go wrapper functions.

//go:wasmimport spacetime_10.0 table_id_from_name
func rawTableIdFromName(namePtr *byte, nameLen uint32, out *uint32) uint32

//go:wasmimport spacetime_10.0 datastore_table_row_count
func rawDatastoreTableRowCount(tableId uint32, out *uint64) uint32

//go:wasmimport spacetime_10.0 datastore_table_scan_bsatn
func rawDatastoreTableScanBSATN(tableId uint32, out *uint32) uint32

//go:wasmimport spacetime_10.0 datastore_insert_bsatn
func rawDatastoreInsertBSATN(tableId uint32, rowPtr *byte, rowLenPtr *uint32) uint32

//go:wasmimport spacetime_10.0 datastore_update_bsatn
func rawDatastoreUpdateBSATN(tableId uint32, indexId uint32, rowPtr *byte, rowLenPtr *uint32) uint32

//go:wasmimport spacetime_10.0 datastore_delete_all_by_eq_bsatn
func rawDatastoreDeleteAllByEqBSATN(tableId uint32, relPtr *byte, relLen uint32, out *uint32) uint32

//go:wasmimport spacetime_10.0 index_id_from_name
func rawIndexIdFromName(namePtr *byte, nameLen uint32, out *uint32) uint32

//go:wasmimport spacetime_10.0 datastore_index_scan_range_bsatn
func rawDatastoreIndexScanRangeBSATN(indexId uint32, prefixPtr *byte, prefixLen uint32, prefixElems uint32, rstartPtr *byte, rstartLen uint32, rendPtr *byte, rendLen uint32, out *uint32) uint32

//go:wasmimport spacetime_10.0 datastore_delete_by_index_scan_range_bsatn
func rawDatastoreDeleteByIndexScanRangeBSATN(indexId uint32, prefixPtr *byte, prefixLen uint32, prefixElems uint32, rstartPtr *byte, rstartLen uint32, rendPtr *byte, rendLen uint32, out *uint32) uint32

//go:wasmimport spacetime_10.0 row_iter_bsatn_advance
func rawRowIterBSATNAdvance(iter uint32, bufPtr *byte, bufLenPtr *uint32) int32

//go:wasmimport spacetime_10.0 row_iter_bsatn_close
func rawRowIterBSATNClose(iter uint32) uint32

//go:wasmimport spacetime_10.0 bytes_source_read
func rawBytesSourceRead(source uint32, bufPtr *byte, bufLenPtr *uint32) int32

//go:wasmimport spacetime_10.0 bytes_sink_write
func rawBytesSinkWrite(sink uint32, bufPtr *byte, bufLenPtr *uint32) uint32

//go:wasmimport spacetime_10.0 console_log
func rawConsoleLog(level uint32, targetPtr *byte, targetLen uint32, fnPtr *byte, fnLen uint32, line uint32, msgPtr *byte, msgLen uint32)

//go:wasmimport spacetime_10.0 console_timer_start
func rawConsoleTimerStart(namePtr *byte, nameLen uint32) uint32

//go:wasmimport spacetime_10.0 console_timer_end
func rawConsoleTimerEnd(timerId uint32) uint32

//go:wasmimport spacetime_10.0 identity
func rawIdentity(outPtr *byte)

// spacetime_10.1

//go:wasmimport spacetime_10.1 bytes_source_remaining_length
func rawBytesSourceRemainingLength(source uint32, out *uint32) int32

// spacetime_10.3

//go:wasmimport spacetime_10.3 procedure_start_mut_tx
func rawProcedureStartMutTx(out *int64) uint32

//go:wasmimport spacetime_10.3 procedure_commit_mut_tx
func rawProcedureCommitMutTx() uint32

//go:wasmimport spacetime_10.3 procedure_abort_mut_tx
func rawProcedureAbortMutTx() uint32

//go:wasmimport spacetime_10.3 procedure_sleep_until
func rawProcedureSleepUntil(wakeAtMicrosSinceUnixEpoch int64) int64

//go:wasmimport spacetime_10.3 procedure_http_request
func rawProcedureHttpRequest(requestPtr *byte, requestLen uint32, bodyPtr *byte, bodyLen uint32, out *uint32) uint32

// spacetime_10.4

//go:wasmimport spacetime_10.4 datastore_index_scan_point_bsatn
func rawDatastoreIndexScanPointBSATN(indexId uint32, pointPtr *byte, pointLen uint32, out *uint32) uint32

//go:wasmimport spacetime_10.4 datastore_delete_by_index_scan_point_bsatn
func rawDatastoreDeleteByIndexScanPointBSATN(indexId uint32, pointPtr *byte, pointLen uint32, out *uint32) uint32
