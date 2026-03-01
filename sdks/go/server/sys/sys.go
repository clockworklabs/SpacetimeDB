package sys

// Log level constants
const (
	LogLevelError uint8 = 0
	LogLevelWarn  uint8 = 1
	LogLevelInfo  uint8 = 2
	LogLevelDebug uint8 = 3
	LogLevelTrace uint8 = 4
	LogLevelPanic uint8 = 101
)

// TableIdFromName looks up a table ID by name.
func TableIdFromName(name string) (uint32, error) {
	nameBytes := []byte(name)
	var tableId uint32
	ret := rawTableIdFromName(&nameBytes[0], uint32(len(nameBytes)), &tableId)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return 0, err
	}
	return tableId, nil
}

// DatastoreTableRowCount returns the row count for a table.
func DatastoreTableRowCount(tableId uint32) (uint64, error) {
	var count uint64
	ret := rawDatastoreTableRowCount(tableId, &count)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return 0, err
	}
	return count, nil
}

// DatastoreTableScanBSATN starts a scan of all rows in a table.
func DatastoreTableScanBSATN(tableId uint32) (*RowIterator, error) {
	var iterHandle uint32
	ret := rawDatastoreTableScanBSATN(tableId, &iterHandle)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return nil, err
	}
	return NewRowIterator(iterHandle), nil
}

// DatastoreInsertBSATN inserts a BSATN-encoded row and returns updated sequence values.
func DatastoreInsertBSATN(tableId uint32, row []byte) ([]byte, error) {
	// Allocate at least 1 byte so &buf[0] never panics (empty product types encode to 0 bytes).
	buf := make([]byte, max(len(row), 1), max(len(row), 1)+64)
	copy(buf, row)
	bufLen := uint32(len(row))
	ret := rawDatastoreInsertBSATN(tableId, &buf[0], &bufLen)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return nil, err
	}
	return buf[:bufLen], nil
}

// DatastoreUpdateBSATN updates a row by its unique index, returning updated sequence values.
func DatastoreUpdateBSATN(tableId uint32, indexId uint32, row []byte) ([]byte, error) {
	buf := make([]byte, max(len(row), 1), max(len(row), 1)+64)
	copy(buf, row)
	bufLen := uint32(len(row))
	ret := rawDatastoreUpdateBSATN(tableId, indexId, &buf[0], &bufLen)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return nil, err
	}
	return buf[:bufLen], nil
}

// DatastoreDeleteAllByEqBSATN deletes all rows matching BSATN-encoded relation.
func DatastoreDeleteAllByEqBSATN(tableId uint32, rel []byte) (uint32, error) {
	var deleted uint32
	var relPtr *byte
	if len(rel) > 0 {
		relPtr = &rel[0]
	}
	ret := rawDatastoreDeleteAllByEqBSATN(tableId, relPtr, uint32(len(rel)), &deleted)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return 0, err
	}
	return deleted, nil
}

// IndexIdFromName looks up an index ID by name.
func IndexIdFromName(name string) (uint32, error) {
	nameBytes := []byte(name)
	var indexId uint32
	ret := rawIndexIdFromName(&nameBytes[0], uint32(len(nameBytes)), &indexId)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return 0, err
	}
	return indexId, nil
}

// DatastoreIndexScanPointBSATN starts a point scan on an index.
func DatastoreIndexScanPointBSATN(indexId uint32, point []byte) (*RowIterator, error) {
	var iterHandle uint32
	ret := rawDatastoreIndexScanPointBSATN(indexId, &point[0], uint32(len(point)), &iterHandle)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return nil, err
	}
	return NewRowIterator(iterHandle), nil
}

// DatastoreIndexScanRangeBSATN starts a range scan on an index.
func DatastoreIndexScanRangeBSATN(indexId uint32, prefix []byte, prefixElems uint32, rstart []byte, rend []byte) (*RowIterator, error) {
	var iterHandle uint32
	var prefixPtr, rstartPtr, rendPtr *byte
	if len(prefix) > 0 {
		prefixPtr = &prefix[0]
	}
	if len(rstart) > 0 {
		rstartPtr = &rstart[0]
	}
	if len(rend) > 0 {
		rendPtr = &rend[0]
	}
	ret := rawDatastoreIndexScanRangeBSATN(indexId, prefixPtr, uint32(len(prefix)), prefixElems, rstartPtr, uint32(len(rstart)), rendPtr, uint32(len(rend)), &iterHandle)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return nil, err
	}
	return NewRowIterator(iterHandle), nil
}

// DatastoreDeleteByIndexScanPointBSATN deletes rows matching a point scan on an index.
func DatastoreDeleteByIndexScanPointBSATN(indexId uint32, point []byte) (uint32, error) {
	var deleted uint32
	ret := rawDatastoreDeleteByIndexScanPointBSATN(indexId, &point[0], uint32(len(point)), &deleted)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return 0, err
	}
	return deleted, nil
}

// DatastoreDeleteByIndexScanRangeBSATN deletes rows matching a range scan on an index.
func DatastoreDeleteByIndexScanRangeBSATN(indexId uint32, prefix []byte, prefixElems uint32, rstart []byte, rend []byte) (uint32, error) {
	var deleted uint32
	var prefixPtr, rstartPtr, rendPtr *byte
	if len(prefix) > 0 {
		prefixPtr = &prefix[0]
	}
	if len(rstart) > 0 {
		rstartPtr = &rstart[0]
	}
	if len(rend) > 0 {
		rendPtr = &rend[0]
	}
	ret := rawDatastoreDeleteByIndexScanRangeBSATN(indexId, prefixPtr, uint32(len(prefix)), prefixElems, rstartPtr, uint32(len(rstart)), rendPtr, uint32(len(rend)), &deleted)
	if err := errnoFromU16(uint16(ret)); err != nil {
		return 0, err
	}
	return deleted, nil
}

// ConsoleLog logs a message to the host console.
func ConsoleLog(level uint8, target, filename string, line uint32, message string) {
	targetBytes := []byte(target)
	filenameBytes := []byte(filename)
	msgBytes := []byte(message)

	var targetPtr, filenamePtr, msgPtr *byte
	if len(targetBytes) > 0 {
		targetPtr = &targetBytes[0]
	}
	if len(filenameBytes) > 0 {
		filenamePtr = &filenameBytes[0]
	}
	if len(msgBytes) > 0 {
		msgPtr = &msgBytes[0]
	}

	rawConsoleLog(uint32(level), targetPtr, uint32(len(targetBytes)), filenamePtr, uint32(len(filenameBytes)), line, msgPtr, uint32(len(msgBytes)))
}

// ConsoleTimerStart starts a named console timer, returning its ID.
func ConsoleTimerStart(name string) uint32 {
	nameBytes := []byte(name)
	return rawConsoleTimerStart(&nameBytes[0], uint32(len(nameBytes)))
}

// ConsoleTimerEnd ends a console timer by its ID.
func ConsoleTimerEnd(timerId uint32) error {
	ret := rawConsoleTimerEnd(timerId)
	return errnoFromU16(uint16(ret))
}

// GetIdentity returns the module's identity as a 32-byte array.
func GetIdentity() [32]byte {
	var out [32]byte
	rawIdentity(&out[0])
	return out
}
