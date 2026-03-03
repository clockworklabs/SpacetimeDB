package runtime

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
)

// GlobalWriter is a reusable BSATN writer for hot-path encoding.
// Safe because WASM is single-threaded — no concurrent access.
// Used by generated table accessor code.
var GlobalWriter = bsatn.NewWriter(256)

// indexIdCache caches index IDs resolved from the host, keyed by index name.
var indexIdCache = map[string]uint32{}

// GetIndexId returns the index ID for a given index name, caching it on first lookup.
// Used by generated table accessor code.
func GetIndexId(name string) (uint32, error) {
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

// tableIdCache caches table IDs resolved from the host, keyed by table name.
var tableIdCache = map[string]uint32{}

// GetTableId returns the table ID for a given table name, caching it on first lookup.
// Used by generated table accessor code.
func GetTableId(name string) (uint32, error) {
	if id, ok := tableIdCache[name]; ok {
		return id, nil
	}
	id, err := sys.TableIdFromName(name)
	if err != nil {
		return 0, err
	}
	tableIdCache[name] = id
	return id, nil
}

// TableIterator iterates over rows of type T.
type TableIterator[T any] interface {
	Next() (T, bool)
	Close()
}

// DecodeFn decodes a single row of type T from a BSATN reader.
type DecodeFn[T any] func(r bsatn.Reader, v *T) error

// NewTableIterator creates a TableIterator that uses the given decode function.
// Used by generated table accessor code to create iterators with generated decoders.
func NewTableIterator[T any](sysIter *sys.RowIterator, decode DecodeFn[T]) TableIterator[T] {
	return &tableIterator[T]{
		sysIter:  sysIter,
		decodeFn: decode,
	}
}

// tableIterator uses batch reading from the host. The host's row_iter_bsatn_advance
// packs multiple BSATN-encoded rows into a single buffer. We maintain a bsatn.Reader
// over the batch and decode one row at a time, only fetching the next batch when the
// current one is consumed.
type tableIterator[T any] struct {
	sysIter  *sys.RowIterator
	decodeFn DecodeFn[T]
	buf      []byte
	reader   bsatn.Reader
}

func (ti *tableIterator[T]) Next() (T, bool) {
	var zero T
	for {
		// If we have remaining bytes in the current batch, decode one row.
		if ti.reader != nil && ti.reader.Remaining() > 0 {
			var result T
			if err := ti.decodeFn(ti.reader, &result); err != nil {
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

// EncodeKeyInto encodes a single key value into the writer using a type-switch
// for fast encoding of common primitive types.
// Used by generated table accessor code for index lookups.
func EncodeKeyInto[K any](w bsatn.Writer, key K) {
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
	case bsatn.Serializable:
		k.WriteBsatn(w)
	default:
		panic("runtime.EncodeKeyInto: unsupported key type")
	}
}
