package runtime

import (
	"fmt"
	"reflect"
	"unsafe"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
)

// globalWriter is a reusable BSATN writer for hot-path encoding.
// Safe because WASM is single-threaded — no concurrent access.
var globalWriter = bsatn.NewWriter(256)

// indexIdCache caches index IDs resolved from the host, keyed by index name.
var indexIdCache = map[string]uint32{}

// getIndexId returns the index ID for a given index name, caching it on first lookup.
func getIndexId(name string) (uint32, error) {
	if id, ok := indexIdCache[name]; ok {
		return id, nil
	}
	id, err := sys.IndexIdFromName(name)
	if err != nil {
		return 0, err
	}
	indexIdCache[name] = id
	return id, nil
}

// tableHandle caches the resolved table ID for a registered table.
type tableHandle struct {
	reg      *tableRegistration
	tableId  uint32
	resolved bool
}

func (h *tableHandle) resolve() error {
	if h.resolved {
		return nil
	}
	id, err := sys.TableIdFromName(h.reg.name)
	if err != nil {
		return err
	}
	h.tableId = id
	h.resolved = true
	return nil
}

// tableHandles caches resolved table IDs keyed by reflect.Type.
var tableHandles = map[reflect.Type]*tableHandle{}

// getTableHandle returns the table handle for a given Go type, creating and caching it as needed.
func getTableHandle(t reflect.Type) (*tableHandle, error) {
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	if h, ok := tableHandles[t]; ok {
		return h, nil
	}
	for i := range registeredTables {
		reg := &registeredTables[i]
		if reg.goType == t {
			h := &tableHandle{reg: reg}
			tableHandles[t] = h
			return h, nil
		}
	}
	return nil, fmt.Errorf("runtime: no table registered for type %v", t)
}

// getTableHandleFast resolves and returns a table handle for type T.
// Panics on failure. Used by hot-path functions that already expect to panic on error.
func getTableHandleFast[T any](caller string) *tableHandle {
	var zero T
	t := reflect.TypeOf(zero)
	h, err := getTableHandle(t)
	if err != nil {
		panic(fmt.Sprintf("runtime.%s: %v", caller, err))
	}
	if err := h.resolve(); err != nil {
		panic(fmt.Sprintf("runtime.%s: %v", caller, err))
	}
	return h
}

// Insert inserts a row into the table registered for type T.
// Returns the row with any auto-increment fields populated.
// Panics on error (matching Rust SDK behavior). Errors are caught by
// the recover() in wasmCallReducer and reported to the host.
func Insert[T any](row T) T {
	h := getTableHandleFast[T]("Insert")

	// Encode using field plan: unsafe.Pointer(&row) avoids interface boxing.
	globalWriter.Reset()
	h.reg.plan.planEncode(globalWriter, unsafe.Pointer(&row))
	seqBytes, err := sys.DatastoreInsertBSATN(h.tableId, globalWriter.Bytes())
	if err != nil {
		panic(fmt.Sprintf("runtime.Insert: %v", err))
	}
	if len(seqBytes) > 0 {
		var result T
		r := bsatn.NewZeroCopyReader(seqBytes)
		if decErr := h.reg.plan.planDecode(r, unsafe.Pointer(&result)); decErr != nil {
			panic(fmt.Sprintf("runtime.Insert: decode error: %v", decErr))
		}
		return result
	}
	return row
}

// Delete deletes all rows matching the given row from the table registered for type T.
// Panics on error (matching Rust SDK behavior).
func Delete[T any](row T) {
	h := getTableHandleFast[T]("Delete")

	// Encode array header + row in one pass using the global writer.
	globalWriter.Reset()
	globalWriter.PutArrayLen(1)
	h.reg.plan.planEncode(globalWriter, unsafe.Pointer(&row))
	if _, err := sys.DatastoreDeleteAllByEqBSATN(h.tableId, globalWriter.Bytes()); err != nil {
		panic(fmt.Sprintf("runtime.Delete: %v", err))
	}
}

// Scan returns an iterator over all rows in the table registered for type T.
func Scan[T any]() (TableIterator[T], error) {
	var zero T
	t := reflect.TypeOf(zero)
	h, err := getTableHandle(t)
	if err != nil {
		return nil, err
	}
	if err := h.resolve(); err != nil {
		return nil, err
	}

	iter, err := sys.DatastoreTableScanBSATN(h.tableId)
	if err != nil {
		return nil, err
	}
	return &tableIterator[T]{
		sysIter: iter,
		plan:    h.reg.plan,
	}, nil
}

// Count returns the number of rows in the table registered for type T.
func Count[T any]() (uint64, error) {
	var zero T
	t := reflect.TypeOf(zero)
	h, err := getTableHandle(t)
	if err != nil {
		return 0, err
	}
	if err := h.resolve(); err != nil {
		return 0, err
	}

	return sys.DatastoreTableRowCount(h.tableId)
}

// FindBy looks up a row by a unique index.
// The indexName must match the index registered on the table.
// The key is BSATN-encoded and used for a point scan.
func FindBy[T any, K any](indexName string, key K) (T, bool, error) {
	var zero T
	t := reflect.TypeOf(zero)
	h, err := getTableHandle(t)
	if err != nil {
		return zero, false, err
	}
	if err := h.resolve(); err != nil {
		return zero, false, err
	}

	indexId, err := getIndexId(indexName)
	if err != nil {
		return zero, false, err
	}

	// Fast key encoding using type switch.
	globalWriter.Reset()
	encodeKeyInto(globalWriter, key)
	keyBytes := globalWriter.Bytes()

	iter, err := sys.DatastoreIndexScanPointBSATN(indexId, keyBytes)
	if err != nil {
		return zero, false, err
	}
	defer iter.Close()

	data, ok, err := iter.Next()
	if !ok || err != nil {
		return zero, false, err
	}

	// Decode using field plan with zero-copy reader (data is a fresh allocation).
	var result T
	r := bsatn.NewZeroCopyReader(data)
	if decErr := h.reg.plan.planDecode(r, unsafe.Pointer(&result)); decErr != nil {
		return zero, false, decErr
	}
	return result, true, nil
}

// TableIterator iterates over rows of type T.
type TableIterator[T any] interface {
	Next() (T, bool)
	Close()
}

// tableIterator uses batch reading from the host. The host's row_iter_bsatn_advance
// packs multiple BSATN-encoded rows into a single buffer. We maintain a bsatn.Reader
// over the batch and decode one row at a time, only fetching the next batch when the
// current one is consumed.
type tableIterator[T any] struct {
	sysIter *sys.RowIterator
	plan    *structPlan
	buf     []byte
	reader  bsatn.Reader
}

func (ti *tableIterator[T]) Next() (T, bool) {
	var zero T
	for {
		// If we have remaining bytes in the current batch, decode one row.
		if ti.reader != nil && ti.reader.Remaining() > 0 {
			var result T
			if err := ti.plan.planDecode(ti.reader, unsafe.Pointer(&result)); err != nil {
				return zero, false
			}
			return result, true
		}

		// If the host iterator is exhausted, no more rows.
		if ti.sysIter.IsExhausted() {
			return zero, false
		}

		// Fetch the next batch from the host.
		// Release old buffer so GC keeps it alive only via zero-copy string refs.
		// ReadBatch allocates a fresh buffer, enabling zero-copy string decode.
		ti.buf = nil
		if err := ti.sysIter.ReadBatch(&ti.buf); err != nil {
			return zero, false
		}
		if len(ti.buf) == 0 {
			return zero, false
		}
		ti.reader = bsatn.NewZeroCopyReader(ti.buf)
	}
}

func (ti *tableIterator[T]) Close() {
	ti.sysIter.Close()
}

// DeleteBy deletes all rows matching a point scan on the named index.
// Returns the number of deleted rows.
// Panics on error (matching Rust SDK behavior).
func DeleteBy[T any, K any](indexName string, key K) uint32 {
	indexId, err := getIndexId(indexName)
	if err != nil {
		panic(fmt.Sprintf("runtime.DeleteBy: %v", err))
	}

	globalWriter.Reset()
	encodeKeyInto(globalWriter, key)
	deleted, err := sys.DatastoreDeleteByIndexScanPointBSATN(indexId, globalWriter.Bytes())
	if err != nil {
		panic(fmt.Sprintf("runtime.DeleteBy: %v", err))
	}
	return deleted
}

// UpdateBy updates a row identified by the given index.
// The new row replaces the existing row found via the index.
// Returns the row with any auto-increment fields populated.
// Panics on error (matching Rust SDK behavior).
func UpdateBy[T any](indexName string, row T) T {
	h := getTableHandleFast[T]("UpdateBy")

	indexId, err := getIndexId(indexName)
	if err != nil {
		panic(fmt.Sprintf("runtime.UpdateBy: %v", err))
	}

	globalWriter.Reset()
	h.reg.plan.planEncode(globalWriter, unsafe.Pointer(&row))
	seqBytes, err := sys.DatastoreUpdateBSATN(h.tableId, indexId, globalWriter.Bytes())
	if err != nil {
		panic(fmt.Sprintf("runtime.UpdateBy: %v", err))
	}
	if len(seqBytes) > 0 {
		var result T
		r := bsatn.NewZeroCopyReader(seqBytes)
		if decErr := h.reg.plan.planDecode(r, unsafe.Pointer(&result)); decErr != nil {
			panic(fmt.Sprintf("runtime.UpdateBy: decode error: %v", decErr))
		}
		return result
	}
	return row
}

// encodeKeyInto encodes a single key value into the writer using a type-switch
// for fast encoding of common primitive types, falling back to reflect for others.
func encodeKeyInto[K any](w bsatn.Writer, key K) {
	switch k := any(key).(type) {
	case bool:
		w.PutBool(k)
	case uint8:
		w.PutU8(k)
	case uint16:
		w.PutU16(k)
	case uint32:
		w.PutU32(k)
	case uint64:
		w.PutU64(k)
	case int8:
		w.PutI8(k)
	case int16:
		w.PutI16(k)
	case int32:
		w.PutI32(k)
	case int64:
		w.PutI64(k)
	case float32:
		w.PutF32(k)
	case float64:
		w.PutF64(k)
	case string:
		w.PutString(k)
	default:
		// Fall back to reflect-based encoding for complex key types.
		reflectEncodeValue(w, reflect.ValueOf(key))
	}
}
