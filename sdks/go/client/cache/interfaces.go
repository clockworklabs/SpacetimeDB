package cache

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// CallbackID identifies a registered callback.
type CallbackID uint64

// RowDecoder can decode a BSATN row.
type RowDecoder func(r bsatn.Reader) (any, error)

// RowEncoder can encode a row to BSATN bytes for use as a cache key.
type RowEncoder func(row any) []byte

// TableDef defines a table for registration with the cache.
type TableDef interface {
	TableName() string
	DecodeRow(r bsatn.Reader) (any, error)
	EncodeRow(row any) []byte
}

// InsertCallback is called when a row is inserted.
type InsertCallback func(row any)

// DeleteCallback is called when a row is deleted.
type DeleteCallback func(row any)
