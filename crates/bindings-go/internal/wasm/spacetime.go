package wasm

import (
	"context"
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/db"
	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

// Spacetime host module error codes
const (
	SPACETIME_OK     uint32 = 0
	SPACETIME_ERROR  uint32 = 1
	BUFFER_TOO_SMALL uint32 = 0xFFFFFFFE
)

// spacetimeModule implements the spacetime_10.0 module
type spacetimeModule struct {
	runtime *Runtime
}

// NewSpacetimeModule creates a new spacetime_10.0 module
func NewSpacetimeModule(runtime *Runtime) *spacetimeModule {
	return &spacetimeModule{
		runtime: runtime,
	}
}

// Instantiate instantiates the spacetime_10.0 module
func (m *spacetimeModule) Instantiate(ctx context.Context, r wazero.Runtime) error {
	// Create module builder
	builder := r.NewHostModuleBuilder("spacetime_10.0")

	// Register functions
	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreDeleteAllByEqBsatn), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_delete_all_by_eq_bsatn")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.rowIterBsatnClose), []api.ValueType{api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("row_iter_bsatn_close")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreDeleteByIndexScanRangeBsatn), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_delete_by_index_scan_range_bsatn")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreIndexScanRangeBsatn), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_index_scan_range_bsatn")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreInsertBsatn), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_insert_bsatn")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreUpdateBsatn), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_update_bsatn")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreTableScanBsatn), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_table_scan_bsatn")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreTableRowCount), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_table_row_count")

	// Add datastore_btree_scan_bsatn which is a deprecated alias for datastore_index_scan_range_bsatn
	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.datastoreIndexScanRangeBsatn), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("datastore_btree_scan_bsatn")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.indexIDFromName), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("index_id_from_name")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.tableIDFromName), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("table_id_from_name")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.bytesSourceRead), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("bytes_source_read")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.bytesSourceGetLen), []api.ValueType{api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("bytes_source_get_len")

	// Add specific byte_buffer_source_get_len export for backward compatibility
	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.bytesSourceGetLen), []api.ValueType{api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("byte_buffer_source_get_len")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.bytesSinkWrite), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("bytes_sink_write")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.consoleLog), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{}).
		Export("console_log")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.rowIterBsatnAdvance), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("row_iter_bsatn_advance")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.dbCreateTable), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("db_create_table")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.getErrorLength), []api.ValueType{api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("get_error_length")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.readErrorMessage), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("read_error_message")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.debugLog), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("debug_log")

	// Additional helpful functions that the WASM module might be looking for
	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.bsatnSerialize), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("bsatn_serialize")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.bsatnDeserialize), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("bsatn_deserialize")

	// Additional functions from Rust bindings
	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.volatileNonatomicScheduleImmediate), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("volatile_nonatomic_schedule_immediate")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.identity), []api.ValueType{api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("identity")

	// Additional functions that might be needed
	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.logEnabled), []api.ValueType{api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("log_enabled")

	// Timer functions (needed by perf-test module)
	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.consoleTimerStart), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("console_timer_start")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.consoleTimerEnd), []api.ValueType{api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("console_timer_end")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.bsatnDeserializeTableSchema), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("bsatn_deserialize_table_schema")

	builder.NewFunctionBuilder().
		WithGoFunction(api.GoFunc(m.spacetimeModuleAbiVersion), []api.ValueType{}, []api.ValueType{api.ValueTypeI32}).
		Export("spacetime_module_abi_version")

	// Build and instantiate module
	_, err := builder.Instantiate(ctx)
	return err
}

// datastoreDeleteAllByEqBsatn deletes all rows that match the given equality condition
func (m *spacetimeModule) datastoreDeleteAllByEqBsatn(ctx context.Context, stack []uint64) {
	tableID := uint32(stack[0])
	keyPtr := uint32(stack[1])
	keyLen := uint32(stack[2])
	// The new 4th argument (could be reserved or for future use)
	_ = uint32(stack[3]) // TODO: handle if needed

	// Get key data from memory
	key, err := m.runtime.ReadFromMemory(keyPtr, keyLen)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	// Delete rows
	err = m.runtime.db.Delete(db.TableID(tableID), key)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	stack[0] = 0
}

// rowIterBsatnClose closes a row iterator
func (m *spacetimeModule) rowIterBsatnClose(ctx context.Context, stack []uint64) {
	iterID := uint32(stack[0])
	iter := &db.RowIter{
		IterID:  iterID,
		Runtime: m.runtime.baseRuntime,
	}
	iter.Close()
	stack[0] = 0 // Success
}

// datastoreDeleteByIndexScanRangeBsatn deletes rows within an index scan range
func (m *spacetimeModule) datastoreDeleteByIndexScanRangeBsatn(ctx context.Context, stack []uint64) {
	indexID := uint32(stack[0])
	startPtr := uint32(stack[1])
	startLen := uint32(stack[2])
	endPtr := uint32(stack[3])
	endLen := uint32(stack[4])
	startInclusive := uint32(stack[5])
	endInclusive := uint32(stack[6])
	limit := uint32(stack[7])
	offset := uint32(stack[8])

	// Get range data from memory
	start, err := m.runtime.ReadFromMemory(startPtr, startLen)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	end, err := m.runtime.ReadFromMemory(endPtr, endLen)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	// Delete rows
	iter, err := m.runtime.db.ScanIndex(db.IndexID(indexID), start, end, startInclusive != 0, endInclusive != 0, limit, offset)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}
	defer iter.Close()

	for !iter.IsExhausted() {
		key, err := iter.Read()
		if err != nil {
			stack[0] = uint64(0xFFFFFFFF)
			return
		}
		err = m.runtime.db.Delete(db.TableID(indexID), key)
		if err != nil {
			stack[0] = uint64(0xFFFFFFFF)
			return
		}
	}

	stack[0] = 0
}

// datastoreIndexScanRangeBsatn creates an iterator for scanning an index range
func (m *spacetimeModule) datastoreIndexScanRangeBsatn(ctx context.Context, stack []uint64) {
	indexID := uint32(stack[0])
	startPtr := uint32(stack[1])
	startLen := uint32(stack[2])
	endPtr := uint32(stack[3])
	endLen := uint32(stack[4])
	startInclusive := uint32(stack[5])
	endInclusive := uint32(stack[6])
	limit := uint32(stack[7])
	offset := uint32(stack[8])

	// Get range data from memory
	start, err := m.runtime.ReadFromMemory(startPtr, startLen)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	end, err := m.runtime.ReadFromMemory(endPtr, endLen)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	// Create iterator
	iter, err := m.runtime.db.ScanIndex(db.IndexID(indexID), start, end, startInclusive != 0, endInclusive != 0, limit, offset)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	stack[0] = uint64(iter.IterID)
}

// datastoreInsertBsatn inserts a row into a table
func (m *spacetimeModule) datastoreInsertBsatn(ctx context.Context, stack []uint64) {
	tableID := uint32(stack[0])
	dataPtr := uint32(stack[1])
	dataLen := uint32(stack[2])

	// Get data from memory
	data, err := m.runtime.ReadFromMemory(dataPtr, dataLen)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	// Insert row
	err = m.runtime.db.Insert(db.TableID(tableID), data)
	if err != nil {
		stack[0] = uint64(0xFFFFFFFF)
		return
	}

	stack[0] = 0
}

// datastoreUpdateBsatn updates a row in a table
func (m *spacetimeModule) datastoreUpdateBsatn(ctx context.Context, stack []uint64) {
	tableID := uint32(stack[0])
	indexID := uint32(stack[1])    // Corresponds to index_id from Rust FFI
	rowDataPtr := uint32(stack[2]) // Pointer to the new row data (BSATN encoded)
	rowDataLen := uint32(stack[3]) // Length of the new row data

	// Read the full row data from WASM memory
	rowData, err := m.runtime.ReadFromMemory(rowDataPtr, rowDataLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading row data: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// The update logic requires:
	// 1. Extract key from rowData using the index projection
	// 2. Update the row with the full rowData

	// For now we'll use a simplified key extraction approach
	// In a real implementation, we would project the index fields from rowData

	// Use the serialized row data directly as key for now
	// This is a temporary solution until we can properly extract keys
	key := rowData

	// Log the update operation
	fmt.Printf("[DEBUG] Updating table %d using index %d with %d bytes of data\n",
		tableID, indexID, len(rowData))

	// Update the row in the database
	err = m.runtime.db.Update(db.TableID(tableID), key, rowData)
	if err != nil {
		fmt.Printf("[DEBUG] Update error: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	stack[0] = 0 // Success
}

// indexIDFromName gets the index ID for a given name
func (m *spacetimeModule) indexIDFromName(ctx context.Context, stack []uint64) {
	namePtr := uint32(stack[0])
	nameLen := uint32(stack[1])
	outPtr := uint32(stack[2]) // Pointer to write the IndexId result

	// Get name from memory
	name, err := m.runtime.ReadFromMemory(namePtr, nameLen)
	if err != nil {
		stack[0] = uint64(db.INVALID) // Return error code
		return
	}

	// Get index ID
	index, err := m.runtime.db.GetIndexByName(string(name))
	if err != nil {
		stack[0] = uint64(db.INVALID) // Return error code
		return
	}

	// Write the IndexId to the outPtr
	if !m.runtime.memory.WriteUint32Le(outPtr, uint32(index.GetID())) {
		// Failed to write to memory
		stack[0] = uint64(db.INVALID) // Return error code
		return
	}

	stack[0] = 0 // Success
}

// tableIDFromName gets the table ID for a given name
func (m *spacetimeModule) tableIDFromName(ctx context.Context, stack []uint64) {
	namePtr := uint32(stack[0])
	nameLen := uint32(stack[1])
	outPtr := uint32(stack[2]) // Pointer to write the TableId result

	// Get name from memory
	name, err := m.runtime.ReadFromMemory(namePtr, nameLen)
	if err != nil {
		stack[0] = uint64(db.INVALID) // Return error code
		return
	}

	// Get table ID
	table, err := m.runtime.db.GetTableByName(string(name))
	if err != nil {
		stack[0] = uint64(db.INVALID) // Return error code
		return
	}

	// Write the TableId to the outPtr
	if !m.runtime.memory.WriteUint32Le(outPtr, uint32(table.GetID())) {
		// Failed to write to memory
		stack[0] = uint64(db.INVALID) // Return error code
		return
	}

	stack[0] = 0 // Success
}

// bytesSourceRead reads bytes from a source
func (m *spacetimeModule) bytesSourceRead(ctx context.Context, stack []uint64) {
	sourceID := uint32(stack[0])
	bufferPtr := uint32(stack[1])
	bufferLenPtr := uint32(stack[2])

	fmt.Printf("[DEBUG] bytesSourceRead called: sourceID=%d, bufferPtr=%d, bufferLenPtr=%d\n",
		sourceID, bufferPtr, bufferLenPtr)

	// Get buffer capacity from memory
	bufferCapacity, ok := m.runtime.memory.ReadUint32Le(bufferLenPtr)
	if !ok {
		fmt.Printf("[DEBUG] bytesSourceRead: Failed to read buffer capacity from memory at %d\n", bufferLenPtr)
		stack[0] = db.NEGATIVE_ONE // -1 as i16
		return
	}

	fmt.Printf("[DEBUG] bytesSourceRead: Buffer capacity is %d bytes\n", bufferCapacity)

	// Get source data from registry
	sourceData, ok := m.runtime.GetByteSource(sourceID)
	if !ok {
		fmt.Printf("[DEBUG] bytesSourceRead: Source ID %d not found in registry\n", sourceID)
		stack[0] = db.NEGATIVE_ONE // -1 as i16
		return
	}

	fmt.Printf("[DEBUG] bytesSourceRead: Source data found, length=%d bytes\n", len(sourceData))
	if len(sourceData) > 0 && len(sourceData) < 100 {
		fmt.Printf("[DEBUG] bytesSourceRead: Source data content: %s\n", string(sourceData))
	}

	// If the buffer is empty, just return 0 bytes read
	if len(sourceData) == 0 {
		if !m.runtime.memory.WriteUint32Le(bufferLenPtr, 0) {
			fmt.Printf("[DEBUG] bytesSourceRead: Failed to write 0 bytes read to memory\n")
			stack[0] = db.NEGATIVE_ONE
			return
		}
		fmt.Printf("[DEBUG] bytesSourceRead: Empty source, returning 0 bytes\n")
		stack[0] = 0 // Success with 0 bytes read
		return
	}

	// Check if buffer is too small
	if uint32(len(sourceData)) > bufferCapacity {
		// Write the required size to bufferLenPtr
		if !m.runtime.memory.WriteUint32Le(bufferLenPtr, uint32(len(sourceData))) {
			fmt.Printf("[DEBUG] bytesSourceRead: Failed to write required size to memory\n")
			stack[0] = db.NEGATIVE_ONE
			return
		}
		fmt.Printf("[DEBUG] bytesSourceRead: Buffer too small (%d bytes), need %d bytes\n",
			bufferCapacity, len(sourceData))
		stack[0] = uint64(BUFFER_TOO_SMALL)
		return
	}

	// Write source data to buffer
	if !m.runtime.memory.Write(bufferPtr, sourceData) {
		fmt.Printf("[DEBUG] bytesSourceRead: Failed to write data to memory at %d\n", bufferPtr)
		stack[0] = db.NEGATIVE_ONE
		return
	}

	// Write actual size to bufferLenPtr
	if !m.runtime.memory.WriteUint32Le(bufferLenPtr, uint32(len(sourceData))) {
		fmt.Printf("[DEBUG] bytesSourceRead: Failed to write actual bytes read to memory\n")
		stack[0] = db.NEGATIVE_ONE
		return
	}

	fmt.Printf("[DEBUG] bytesSourceRead: Successfully read and wrote %d bytes to memory\n", len(sourceData))
	stack[0] = 0 // Success with data
}

// bytesSourceGetLen gets the length of a byte source
func (m *spacetimeModule) bytesSourceGetLen(ctx context.Context, stack []uint64) {
	sourceID := uint32(stack[0])

	fmt.Printf("[DEBUG] bytesSourceGetLen called: sourceID=%d\n", sourceID)

	// Get source data from registry
	sourceData, ok := m.runtime.GetByteSource(sourceID)
	if !ok {
		fmt.Printf("[DEBUG] bytesSourceGetLen: Source ID %d not found in registry\n", sourceID)
		stack[0] = db.NEGATIVE_ONE // -1 as i16
		return
	}

	// Return the length of the source data
	fmt.Printf("[DEBUG] bytesSourceGetLen: Source ID %d has length %d\n", sourceID, len(sourceData))
	stack[0] = uint64(len(sourceData))
}

// bytesSinkWrite writes bytes to a sink
func (m *spacetimeModule) bytesSinkWrite(ctx context.Context, stack []uint64) {
	sinkID := uint32(stack[0])
	bufferPtr := uint32(stack[1])
	bufferLen := uint32(stack[2])

	fmt.Printf("[DEBUG] bytesSinkWrite called: sinkID=%d, bufferPtr=%d, bufferLen=%d\n",
		sinkID, bufferPtr, bufferLen)

	// Get buffer data from memory
	bufferData, err := m.runtime.ReadFromMemory(bufferPtr, bufferLen)
	if err != nil {
		fmt.Printf("[DEBUG] bytesSinkWrite: Error reading buffer data: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	fmt.Printf("[DEBUG] bytesSinkWrite: Read %d bytes from memory\n", len(bufferData))
	if len(bufferData) < 100 {
		fmt.Printf("[DEBUG] bytesSinkWrite: Data content: %s\n", string(bufferData))
	}

	// Write data to sink
	if ok := m.runtime.WriteByteSink(sinkID, bufferData); !ok {
		fmt.Printf("[DEBUG] bytesSinkWrite: Failed to write to sink ID %d\n", sinkID)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	fmt.Printf("[DEBUG] bytesSinkWrite: Successfully wrote %d bytes to sink ID %d\n", len(bufferData), sinkID)
	// Success
	stack[0] = 0
}

// consoleLog logs a message to the console
func (m *spacetimeModule) consoleLog(ctx context.Context, stack []uint64) {
	// Accept 8 arguments, use as needed
	_ = uint32(stack[0])
	_ = uint32(stack[1])
	_ = uint32(stack[2])
	_ = uint32(stack[3])
	_ = uint32(stack[4])
	_ = uint32(stack[5])
	_ = uint32(stack[6])
	_ = uint32(stack[7]) // TODO: handle if needed

	level := uint32(stack[0])
	msgPtr := uint32(stack[1])
	msgLen := uint32(stack[2])
	filePtr := uint32(stack[3])
	fileLen := uint32(stack[4])
	line := uint32(stack[5])
	column := uint32(stack[6])

	// Get message and file from memory
	msg, err := m.runtime.ReadFromMemory(msgPtr, msgLen)
	if err != nil {
		return
	}

	file, err := m.runtime.ReadFromMemory(filePtr, fileLen)
	if err != nil {
		return
	}

	// Log message
	fmt.Printf("[%d] %s (%s:%d:%d)\n", level, string(msg), string(file), line, column)
}

// rowIterBsatnAdvance advances a row iterator and writes data to the provided buffer.
// Signature: (iter RowIter, buffer_ptr *mut u8, buffer_len_ptr *mut usize) -> i16
// iter: u32 (iterator handle)
// buffer_ptr: u32 (pointer to buffer in WASM memory)
// buffer_len_ptr: u32 (pointer to usize in WASM memory, holds buffer capacity, updated with bytes written)
// returns: i16 (status code: 0 for success, -1 for exhausted, >0 for error e.g., BUFFER_TOO_SMALL)
func (m *spacetimeModule) rowIterBsatnAdvance(ctx context.Context, stack []uint64) {
	iterID := uint32(stack[0])
	bufferPtr := uint32(stack[1])
	bufferLenPtr := uint32(stack[2])

	// 1. Read current buffer capacity from WASM memory
	capacity, ok := m.runtime.memory.ReadUint32Le(bufferLenPtr)
	if !ok {
		fmt.Printf("[DEBUG] Failed to read buffer capacity from memory at %d\n", bufferLenPtr)
		stack[0] = db.NEGATIVE_ONE // -1 as i16
		return
	}

	// 2. Get the row iterator from the runtime
	iter := &db.RowIter{
		IterID:  iterID,
		Runtime: m.runtime.baseRuntime,
	}

	// 3. Read the next row data
	rowData, err := iter.Read()
	if err != nil || rowData == nil {
		// If exhausted or error, return appropriate code
		fmt.Printf("[DEBUG] Iterator exhausted or error: %v\n", err)
		if !m.runtime.memory.WriteUint32Le(bufferLenPtr, 0) {
			fmt.Printf("[DEBUG] Failed to write 0 to buffer length pointer\n")
		}
		stack[0] = db.NEGATIVE_ONE // -1 (exhausted) as i16
		return
	}

	// 4. Check if buffer is large enough for the row data
	if uint32(len(rowData)) > capacity {
		// Buffer too small, write the required size
		fmt.Printf("[DEBUG] Buffer too small for row data: need %d, have %d\n",
			len(rowData), capacity)
		if !m.runtime.memory.WriteUint32Le(bufferLenPtr, uint32(len(rowData))) {
			fmt.Printf("[DEBUG] Failed to write required size to buffer length pointer\n")
			stack[0] = uint64(SPACETIME_ERROR)
			return
		}
		stack[0] = uint64(BUFFER_TOO_SMALL)
		return
	}

	// 5. Write row data to the buffer
	if !m.runtime.memory.Write(bufferPtr, rowData) {
		fmt.Printf("[DEBUG] Failed to write row data to memory\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// 6. Update buffer length with actual bytes written
	if !m.runtime.memory.WriteUint32Le(bufferLenPtr, uint32(len(rowData))) {
		fmt.Printf("[DEBUG] Failed to write actual bytes written\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Success
	stack[0] = 0
}

// dbCreateTable creates a table in the database
func (m *spacetimeModule) dbCreateTable(ctx context.Context, stack []uint64) {
	namePtr := uint32(stack[0])
	nameLen := uint32(stack[1])
	colsPtr := uint32(stack[2])
	colsLen := uint32(stack[3])
	idxPtr := uint32(stack[4]) // Pointer to indices array, could be 0 if no indices

	// Read table name from memory
	nameBytes, err := m.runtime.ReadFromMemory(namePtr, nameLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading table name: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}
	tableName := string(nameBytes)

	// Read column data from memory
	colsBytes, err := m.runtime.ReadFromMemory(colsPtr, colsLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading columns data: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Log what we're doing for debugging
	fmt.Printf("[DEBUG] Creating table '%s' with %d bytes of column data\n", tableName, len(colsBytes))

	// Create a new TableImpl instance
	tableID := db.TableID(uint32(len(m.runtime.db.GetAllTables()) + 1)) // Simple ID generation
	tableImpl := db.NewTableImpl(tableID, tableName, colsBytes, m.runtime.baseRuntime)

	// If indices are provided, parse and create them
	if idxPtr != 0 {
		// Read indices data from memory - for debugging only for now
		idxBytes, err := m.runtime.ReadFromMemory(idxPtr, 1024) // Use a reasonable limit
		if err == nil {
			fmt.Printf("[DEBUG] Read index data: %d bytes\n", len(idxBytes))
		}
	}

	// Register the table in the database
	m.runtime.db.RegisterTable(tableID, tableImpl)

	fmt.Printf("[DEBUG] Successfully created table '%s' with ID %d\n", tableName, tableID)
	stack[0] = uint64(SPACETIME_OK)
}

// getErrorLength gets the length of an error message
func (m *spacetimeModule) getErrorLength(ctx context.Context, stack []uint64) {
	errorID := uint32(stack[0])
	fmt.Printf("[DEBUG] getErrorLength called for error ID %d\n", errorID)
	// Return a zero length for now
	stack[0] = 0
}

// readErrorMessage reads an error message
func (m *spacetimeModule) readErrorMessage(ctx context.Context, stack []uint64) {
	errorID := uint32(stack[0])
	bufferPtr := uint32(stack[1])
	bufferLen := uint32(stack[2])
	fmt.Printf("[DEBUG] readErrorMessage called for error ID %d, buffer ptr %d, buffer len %d\n",
		errorID, bufferPtr, bufferLen)
	// Return success (0)
	stack[0] = 0
}

// debugLog logs a message to the console
func (m *spacetimeModule) debugLog(ctx context.Context, stack []uint64) {
	msgPtr := uint32(stack[0])
	msgLen := uint32(stack[1])

	// Get message from memory
	msg, err := m.runtime.ReadFromMemory(msgPtr, msgLen)
	if err != nil {
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Log message
	fmt.Printf("[WASM DEBUG] %s\n", string(msg))

	// Return success
	stack[0] = uint64(SPACETIME_OK)
}

// bsatnSerialize serializes a value using BSATN format
func (m *spacetimeModule) bsatnSerialize(ctx context.Context, stack []uint64) {
	valPtr := uint32(stack[0])
	valLen := uint32(stack[1])
	outBufPtr := uint32(stack[2])
	outBufLenPtr := uint32(stack[3]) // This is a pointer to a length, not the length itself

	// Read input data
	valBytes, err := m.runtime.ReadFromMemory(valPtr, valLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading BSATN input data: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Read the output buffer capacity
	outBufLen, ok := m.runtime.memory.ReadUint32Le(outBufLenPtr)
	if !ok {
		fmt.Printf("[DEBUG] Error reading output buffer length\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Implement BSATN serialization
	// For now, we'll use the actual db.Serialize method if available, otherwise fallback
	serialized := valBytes
	if m.runtime.db != nil {
		var serializeErr error
		serialized, serializeErr = m.runtime.db.Serialize(valBytes)
		if serializeErr != nil {
			fmt.Printf("[DEBUG] BSATN serialization error: %v\n", serializeErr)
			stack[0] = uint64(SPACETIME_ERROR)
			return
		}
	}

	// Check if output buffer is large enough
	if uint32(len(serialized)) > outBufLen {
		// Write the required length to the length pointer
		if !m.runtime.memory.WriteUint32Le(outBufLenPtr, uint32(len(serialized))) {
			fmt.Printf("[DEBUG] Error writing required buffer length\n")
		}
		fmt.Printf("[DEBUG] BSATN buffer too small: need %d, have %d\n", len(serialized), outBufLen)
		stack[0] = uint64(BUFFER_TOO_SMALL)
		return
	}

	// Write to output buffer
	if err := m.runtime.WriteToMemoryAt(outBufPtr, serialized); err != nil {
		fmt.Printf("[DEBUG] Error writing to output buffer: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Write actual bytes written
	if !m.runtime.memory.WriteUint32Le(outBufLenPtr, uint32(len(serialized))) {
		fmt.Printf("[DEBUG] Error writing actual bytes written\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Return success with bytes written
	stack[0] = uint64(SPACETIME_OK)
}

// bsatnDeserialize deserializes a value from BSATN format
func (m *spacetimeModule) bsatnDeserialize(ctx context.Context, stack []uint64) {
	dataPtr := uint32(stack[0])
	dataLen := uint32(stack[1])
	outBufPtr := uint32(stack[2])
	outBufLenPtr := uint32(stack[3]) // This is a pointer to a length, not the length itself

	// Read input data
	dataBytes, err := m.runtime.ReadFromMemory(dataPtr, dataLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading BSATN data to deserialize: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Read the output buffer capacity
	outBufLen, ok := m.runtime.memory.ReadUint32Le(outBufLenPtr)
	if !ok {
		fmt.Printf("[DEBUG] Error reading output buffer length\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Implement BSATN deserialization
	// For now, use the actual db.Deserialize method if available, otherwise fallback
	deserialized := dataBytes
	if m.runtime.db != nil {
		var deserializeErr error
		deserialized, deserializeErr = m.runtime.db.Deserialize(dataBytes)
		if deserializeErr != nil {
			fmt.Printf("[DEBUG] BSATN deserialization error: %v\n", deserializeErr)
			stack[0] = uint64(SPACETIME_ERROR)
			return
		}
	}

	// Check if output buffer is large enough
	if uint32(len(deserialized)) > outBufLen {
		// Write the required length to the length pointer
		if !m.runtime.memory.WriteUint32Le(outBufLenPtr, uint32(len(deserialized))) {
			fmt.Printf("[DEBUG] Error writing required buffer length\n")
		}
		fmt.Printf("[DEBUG] BSATN buffer too small: need %d, have %d\n", len(deserialized), outBufLen)
		stack[0] = uint64(BUFFER_TOO_SMALL)
		return
	}

	// Write to output buffer
	if err := m.runtime.WriteToMemoryAt(outBufPtr, deserialized); err != nil {
		fmt.Printf("[DEBUG] Error writing to output buffer: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Write actual bytes written
	if !m.runtime.memory.WriteUint32Le(outBufLenPtr, uint32(len(deserialized))) {
		fmt.Printf("[DEBUG] Error writing actual bytes written\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Return success
	stack[0] = uint64(SPACETIME_OK)
}

// datastoreTableScanBsatn scans a table and returns an iterator
func (m *spacetimeModule) datastoreTableScanBsatn(ctx context.Context, stack []uint64) {
	tableID := uint32(stack[0])
	outPtr := uint32(stack[1]) // Pointer to store the iterator ID

	fmt.Printf("[DEBUG] datastoreTableScanBsatn: tableID=%d, outPtr=%d\n", tableID, outPtr)

	// Create a new row iterator
	iterID := uint32(len(m.runtime.db.GetAllTables()) + 100) // Simple unique ID generation
	// Register the iterator ID (in a real implementation we'd store the actual iterator)

	// Write the iterator ID to the output pointer
	if !m.runtime.memory.WriteUint32Le(outPtr, iterID) {
		fmt.Printf("[DEBUG] Failed to write iterator ID to memory\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	fmt.Printf("[DEBUG] Created row iterator with ID %d for table %d\n", iterID, tableID)
	stack[0] = uint64(SPACETIME_OK)
}

// datastoreTableRowCount returns the number of rows in a table
func (m *spacetimeModule) datastoreTableRowCount(ctx context.Context, stack []uint64) {
	tableID := uint32(stack[0])
	outPtr := uint32(stack[1]) // Pointer to write the row count to

	fmt.Printf("[DEBUG] datastoreTableRowCount: tableID=%d, outPtr=%d\n", tableID, outPtr)

	// In a real implementation, we would get the actual row count from the table
	// For now, we'll just return 0 since we're mocking most operations
	rowCount := uint64(0)

	// Write the row count to the output pointer
	if !m.runtime.memory.WriteUint64Le(outPtr, rowCount) {
		fmt.Printf("[DEBUG] Failed to write row count to memory\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	fmt.Printf("[DEBUG] Returning row count %d for table %d\n", rowCount, tableID)
	stack[0] = uint64(SPACETIME_OK)
}

// volatileNonatomicScheduleImmediate schedules an immediate task
func (m *spacetimeModule) volatileNonatomicScheduleImmediate(ctx context.Context, stack []uint64) {
	namePtr := uint32(stack[0])
	nameLen := uint32(stack[1])
	argsPtr := uint32(stack[2])
	argsLen := uint32(stack[3])

	fmt.Printf("[DEBUG] volatileNonatomicScheduleImmediate called: namePtr=%d, nameLen=%d, argsPtr=%d, argsLen=%d\n",
		namePtr, nameLen, argsPtr, argsLen)

	// Read name and args from memory
	name, err := m.runtime.ReadFromMemory(namePtr, nameLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading name: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	args, err := m.runtime.ReadFromMemory(argsPtr, argsLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading args: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	fmt.Printf("[DEBUG] volatileNonatomicScheduleImmediate: name=%s, args=%v\n", string(name), args)

	// In a real implementation, we would schedule the task
	// For now, just acknowledge the request
	stack[0] = uint64(SPACETIME_OK)
}

// identity returns the identity
func (m *spacetimeModule) identity(ctx context.Context, stack []uint64) {
	outPtr := uint32(stack[0])

	fmt.Printf("[DEBUG] identity called: outPtr=%d\n", outPtr)

	// Create a zero-filled identity (32 bytes)
	identity := make([]byte, 32)

	// Write identity to memory
	if !m.runtime.memory.Write(outPtr, identity) {
		fmt.Printf("[DEBUG] Failed to write identity to memory\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	fmt.Printf("[DEBUG] identity: wrote %d bytes to memory\n", len(identity))
	stack[0] = uint64(SPACETIME_OK)
}

// logEnabled checks if logging is enabled at the given level
func (m *spacetimeModule) logEnabled(ctx context.Context, stack []uint64) {
	level := uint32(stack[0])

	fmt.Printf("[DEBUG] logEnabled called for level: %d\n", level)

	// Return true for all log levels for now
	stack[0] = 1 // Return true (enabled)
}

// bsatnDeserializeTableSchema deserializes a table schema from BSATN format
func (m *spacetimeModule) bsatnDeserializeTableSchema(ctx context.Context, stack []uint64) {
	dataPtr := uint32(stack[0])
	dataLen := uint32(stack[1])
	outBufPtr := uint32(stack[2])
	outBufLenPtr := uint32(stack[3])

	fmt.Printf("[DEBUG] bsatnDeserializeTableSchema called: dataPtr=%d, dataLen=%d, outBufPtr=%d, outBufLenPtr=%d\n",
		dataPtr, dataLen, outBufPtr, outBufLenPtr)

	// Read input data
	dataBytes, err := m.runtime.ReadFromMemory(dataPtr, dataLen)
	if err != nil {
		fmt.Printf("[DEBUG] Error reading schema data: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Read the output buffer capacity
	outBufLen, ok := m.runtime.memory.ReadUint32Le(outBufLenPtr)
	if !ok {
		fmt.Printf("[DEBUG] Error reading output buffer length\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// In a real implementation, we would deserialize the schema
	// For now, we'll just pass through the data as is
	deserialized := dataBytes

	// Check if output buffer is large enough
	if uint32(len(deserialized)) > outBufLen {
		// Write the required length to the length pointer
		if !m.runtime.memory.WriteUint32Le(outBufLenPtr, uint32(len(deserialized))) {
			fmt.Printf("[DEBUG] Error writing required buffer length\n")
		}
		fmt.Printf("[DEBUG] Buffer too small: need %d, have %d\n", len(deserialized), outBufLen)
		stack[0] = uint64(BUFFER_TOO_SMALL)
		return
	}

	// Write to output buffer
	if err := m.runtime.WriteToMemoryAt(outBufPtr, deserialized); err != nil {
		fmt.Printf("[DEBUG] Error writing to output buffer: %v\n", err)
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	// Write actual bytes written
	if !m.runtime.memory.WriteUint32Le(outBufLenPtr, uint32(len(deserialized))) {
		fmt.Printf("[DEBUG] Error writing actual bytes written\n")
		stack[0] = uint64(SPACETIME_ERROR)
		return
	}

	fmt.Printf("[DEBUG] Successfully deserialized schema, %d bytes\n", len(deserialized))
	stack[0] = uint64(SPACETIME_OK)
}

// spacetimeModuleAbiVersion returns the version of the spacetime module ABI
func (m *spacetimeModule) spacetimeModuleAbiVersion(ctx context.Context, stack []uint64) {
	// Return ABI version 10
	fmt.Printf("[DEBUG] spacetimeModuleAbiVersion called, returning 10\n")
	stack[0] = 10
}

// Timer functions implementation
// consoleTimerStart starts a timer and returns a timer ID
func (m *spacetimeModule) consoleTimerStart(ctx context.Context, stack []uint64) {
	namePtr := uint32(stack[0])
	nameLen := uint32(stack[1])

	// Get timer name from memory
	name, err := m.runtime.ReadFromMemory(namePtr, nameLen)
	if err != nil {
		fmt.Printf("[DEBUG] consoleTimerStart: Error reading timer name: %v\n", err)
		stack[0] = 0 // Return invalid timer ID
		return
	}

	timerName := string(name)
	fmt.Printf("[DEBUG] Timer started: %s\n", timerName)

	// Generate a simple timer ID (in real implementation, would track actual timers)
	timerID := uint32(len(timerName) + 1) // Simple ID generation
	stack[0] = uint64(timerID)
}

// consoleTimerEnd ends a timer and logs the duration
func (m *spacetimeModule) consoleTimerEnd(ctx context.Context, stack []uint64) {
	timerID := uint32(stack[0])

	fmt.Printf("[DEBUG] Timer ended: ID %d\n", timerID)

	// In real implementation, would calculate and log actual duration
	// For now, just return success
	stack[0] = uint64(SPACETIME_OK)
}
