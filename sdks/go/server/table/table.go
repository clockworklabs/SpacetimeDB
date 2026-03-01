package table

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// TableId is a numeric handle for a SpacetimeDB table.
type TableId uint32

// Iterator iterates over rows of type R.
type Iterator[R any] interface {
	Next() (R, bool)
	Close()
}

// Table provides operations on a SpacetimeDB table.
// WASM execution is single-threaded, so no context is needed for table operations.
type Table[R any] interface {
	TableId() TableId
	Insert(row R) (R, error)
	Delete(row R) error
	Scan() (Iterator[R], error)
	Count() (uint64, error)
}

// EncodeFn encodes a row to BSATN bytes.
type EncodeFn[R any] func(R) []byte

// DecodeFn decodes BSATN bytes to a row.
type DecodeFn[R any] func(bsatn.Reader) (R, error)
