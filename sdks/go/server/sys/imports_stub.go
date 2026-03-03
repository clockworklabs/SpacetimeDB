//go:build !wasip1

package sys

// Stubs for native testing - these panic when called since they require WASM runtime.
// Signatures must match imports.go (using widened uint32/int32 types).

func rawTableIdFromName(namePtr *byte, nameLen uint32, out *uint32) uint32 {
	panic("rawTableIdFromName: not available outside WASM")
}

func rawDatastoreTableRowCount(tableId uint32, out *uint64) uint32 {
	panic("rawDatastoreTableRowCount: not available outside WASM")
}

func rawDatastoreTableScanBSATN(tableId uint32, out *uint32) uint32 {
	panic("rawDatastoreTableScanBSATN: not available outside WASM")
}

func rawDatastoreInsertBSATN(tableId uint32, rowPtr *byte, rowLenPtr *uint32) uint32 {
	panic("rawDatastoreInsertBSATN: not available outside WASM")
}

func rawDatastoreUpdateBSATN(tableId uint32, indexId uint32, rowPtr *byte, rowLenPtr *uint32) uint32 {
	panic("rawDatastoreUpdateBSATN: not available outside WASM")
}

func rawDatastoreDeleteAllByEqBSATN(tableId uint32, relPtr *byte, relLen uint32, out *uint32) uint32 {
	panic("rawDatastoreDeleteAllByEqBSATN: not available outside WASM")
}

func rawIndexIdFromName(namePtr *byte, nameLen uint32, out *uint32) uint32 {
	panic("rawIndexIdFromName: not available outside WASM")
}

func rawDatastoreIndexScanRangeBSATN(indexId uint32, prefixPtr *byte, prefixLen uint32, prefixElems uint32, rstartPtr *byte, rstartLen uint32, rendPtr *byte, rendLen uint32, out *uint32) uint32 {
	panic("rawDatastoreIndexScanRangeBSATN: not available outside WASM")
}

func rawDatastoreDeleteByIndexScanRangeBSATN(indexId uint32, prefixPtr *byte, prefixLen uint32, prefixElems uint32, rstartPtr *byte, rstartLen uint32, rendPtr *byte, rendLen uint32, out *uint32) uint32 {
	panic("rawDatastoreDeleteByIndexScanRangeBSATN: not available outside WASM")
}

func rawRowIterBSATNAdvance(iter uint32, bufPtr *byte, bufLenPtr *uint32) int32 {
	panic("rawRowIterBSATNAdvance: not available outside WASM")
}

func rawRowIterBSATNClose(iter uint32) uint32 {
	panic("rawRowIterBSATNClose: not available outside WASM")
}

func rawBytesSourceRead(source uint32, bufPtr *byte, bufLenPtr *uint32) int32 {
	panic("rawBytesSourceRead: not available outside WASM")
}

func rawBytesSinkWrite(sink uint32, bufPtr *byte, bufLenPtr *uint32) uint32 {
	panic("rawBytesSinkWrite: not available outside WASM")
}

func rawConsoleLog(level uint32, targetPtr *byte, targetLen uint32, fnPtr *byte, fnLen uint32, line uint32, msgPtr *byte, msgLen uint32) {
	panic("rawConsoleLog: not available outside WASM")
}

func rawConsoleTimerStart(namePtr *byte, nameLen uint32) uint32 {
	panic("rawConsoleTimerStart: not available outside WASM")
}

func rawConsoleTimerEnd(timerId uint32) uint32 {
	panic("rawConsoleTimerEnd: not available outside WASM")
}

func rawIdentity(outPtr *byte) {
	panic("rawIdentity: not available outside WASM")
}

func rawBytesSourceRemainingLength(source uint32, out *uint32) int32 {
	panic("rawBytesSourceRemainingLength: not available outside WASM")
}

func rawProcedureStartMutTx(out *int64) uint32 {
	panic("rawProcedureStartMutTx: not available outside WASM")
}

func rawProcedureCommitMutTx() uint32 {
	panic("rawProcedureCommitMutTx: not available outside WASM")
}

func rawProcedureAbortMutTx() uint32 {
	panic("rawProcedureAbortMutTx: not available outside WASM")
}

func rawProcedureSleepUntil(wakeAtMicrosSinceUnixEpoch int64) int64 {
	panic("rawProcedureSleepUntil: not available outside WASM")
}

func rawProcedureHttpRequest(requestPtr *byte, requestLen uint32, bodyPtr *byte, bodyLen uint32, out *uint32) uint32 {
	panic("rawProcedureHttpRequest: not available outside WASM")
}

func rawDatastoreIndexScanPointBSATN(indexId uint32, pointPtr *byte, pointLen uint32, out *uint32) uint32 {
	panic("rawDatastoreIndexScanPointBSATN: not available outside WASM")
}

func rawDatastoreDeleteByIndexScanPointBSATN(indexId uint32, pointPtr *byte, pointLen uint32, out *uint32) uint32 {
	panic("rawDatastoreDeleteByIndexScanPointBSATN: not available outside WASM")
}
