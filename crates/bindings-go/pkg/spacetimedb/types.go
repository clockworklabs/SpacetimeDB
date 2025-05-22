package spacetimedb

import "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/db"

// TableID represents a unique identifier for a table
type TableID uint32

// IndexID represents a unique identifier for an index
type IndexID uint32

// Column represents a table column definition
type Column struct {
	Name string
	Type string
}

// Index represents a table index definition
type Index struct {
	Name    string
	Columns []string
	Unique  bool
}

// RowIter wraps the internal iterator
type RowIter struct {
	inner *db.RowIter
}

// Read reads the next row from the iterator
func (iter *RowIter) Read() ([]byte, error) {
	return iter.inner.Read()
}

// IsExhausted checks if the iterator is exhausted
func (iter *RowIter) IsExhausted() bool {
	return iter.inner.IsExhausted()
}

// Close closes the iterator
func (iter *RowIter) Close() error {
	return iter.inner.Close()
}

// Errno represents an error code
type Errno uint32

// Constants for error codes
const (
	ErrnoSuccess Errno = iota
	ErrnoNotInTransaction
	ErrnoNoSuchTable
	ErrnoNoSuchIndex
	ErrnoWrongIndexAlgo
	ErrnoBSATNDecodeError
	ErrnoMemoryExhausted
	ErrnoOutOfBounds
)

// Constants for inclusive/exclusive bounds
const (
	Exclusive uint32 = 0
	Inclusive uint32 = 1
)
