package runtime

import (
	"fmt"
	"reflect"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
)

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

// Insert inserts a row into the table registered for type T.
// Returns the row with any auto-increment fields populated.
// Panics on error (matching Rust SDK behavior). Errors are caught by
// the recover() in wasmCallReducer and reported to the host.
func Insert[T any](row T) T {
	var zero T
	t := reflect.TypeOf(zero)
	h, err := getTableHandle(t)
	if err != nil {
		panic(fmt.Sprintf("runtime.Insert: %v", err))
	}
	if err := h.resolve(); err != nil {
		panic(fmt.Sprintf("runtime.Insert: %v", err))
	}

	rowBytes := h.reg.encodeFn(row)
	seqBytes, err := sys.DatastoreInsertBSATN(h.tableId, rowBytes)
	if err != nil {
		panic(fmt.Sprintf("runtime.Insert: %v", err))
	}
	if len(seqBytes) > 0 {
		decoded, decErr := h.reg.decodeFn(seqBytes)
		if decErr != nil {
			panic(fmt.Sprintf("runtime.Insert: decode error: %v", decErr))
		}
		return decoded.(T)
	}
	return row
}

// Delete deletes all rows matching the given row from the table registered for type T.
// Panics on error (matching Rust SDK behavior).
func Delete[T any](row T) {
	var zero T
	t := reflect.TypeOf(zero)
	h, err := getTableHandle(t)
	if err != nil {
		panic(fmt.Sprintf("runtime.Delete: %v", err))
	}
	if err := h.resolve(); err != nil {
		panic(fmt.Sprintf("runtime.Delete: %v", err))
	}

	rowBytes := h.reg.encodeFn(row)
	// The host expects a BSATN array of product values: u32 length prefix + elements.
	// Wrap the single row in an array with length = 1.
	w := bsatn.NewWriter(4 + len(rowBytes))
	w.PutArrayLen(1)
	w.PutBytes(rowBytes)
	if _, err := sys.DatastoreDeleteAllByEqBSATN(h.tableId, w.Bytes()); err != nil {
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
		sysIter:  iter,
		decodeFn: h.reg.decodeFn,
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

	indexId, err := sys.IndexIdFromName(indexName)
	if err != nil {
		return zero, false, err
	}

	keyBytes := reflectEncode(key)
	iter, err := sys.DatastoreIndexScanPointBSATN(indexId, keyBytes)
	if err != nil {
		return zero, false, err
	}
	defer iter.Close()

	data, ok, err := iter.Next()
	if !ok || err != nil {
		return zero, false, err
	}

	decoded, decErr := h.reg.decodeFn(data)
	if decErr != nil {
		return zero, false, decErr
	}
	return decoded.(T), true, nil
}

// TableIterator iterates over rows of type T.
type TableIterator[T any] interface {
	Next() (T, bool)
	Close()
}

type tableIterator[T any] struct {
	sysIter  *sys.RowIterator
	decodeFn func(data []byte) (any, error)
}

func (ti *tableIterator[T]) Next() (T, bool) {
	var zero T
	data, ok, err := ti.sysIter.Next()
	if !ok || err != nil {
		return zero, false
	}

	// The decodeFn expects raw BSATN bytes (creates its own reader internally).
	decoded, decErr := ti.decodeFn(data)
	if decErr != nil {
		return zero, false
	}
	return decoded.(T), true
}

func (ti *tableIterator[T]) Close() {
	ti.sysIter.Close()
}

// DeleteBy deletes all rows matching a point scan on the named index.
// Returns the number of deleted rows.
// Panics on error (matching Rust SDK behavior).
func DeleteBy[T any, K any](indexName string, key K) uint32 {
	indexId, err := sys.IndexIdFromName(indexName)
	if err != nil {
		panic(fmt.Sprintf("runtime.DeleteBy: %v", err))
	}

	keyBytes := reflectEncode(key)
	deleted, err := sys.DatastoreDeleteByIndexScanPointBSATN(indexId, keyBytes)
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
	var zero T
	t := reflect.TypeOf(zero)
	h, err := getTableHandle(t)
	if err != nil {
		panic(fmt.Sprintf("runtime.UpdateBy: %v", err))
	}
	if err := h.resolve(); err != nil {
		panic(fmt.Sprintf("runtime.UpdateBy: %v", err))
	}

	indexId, err := sys.IndexIdFromName(indexName)
	if err != nil {
		panic(fmt.Sprintf("runtime.UpdateBy: %v", err))
	}

	rowBytes := h.reg.encodeFn(row)
	seqBytes, err := sys.DatastoreUpdateBSATN(h.tableId, indexId, rowBytes)
	if err != nil {
		panic(fmt.Sprintf("runtime.UpdateBy: %v", err))
	}
	if len(seqBytes) > 0 {
		decoded, decErr := h.reg.decodeFn(seqBytes)
		if decErr != nil {
			panic(fmt.Sprintf("runtime.UpdateBy: decode error: %v", decErr))
		}
		return decoded.(T)
	}
	return row
}

// encodeKey is a helper to BSATN-encode a single value for index lookups.
func encodeKey(key any) []byte {
	w := bsatn.NewWriter(32)
	rv := reflect.ValueOf(key)
	reflectEncodeValue(w, rv)
	return w.Bytes()
}
