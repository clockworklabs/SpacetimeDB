package spacetimedb

// TableID represents a unique identifier for a table
type TableID uint32

// IndexID represents a unique identifier for an index
type IndexID uint32

// RowIter represents an iterator over table rows
type RowIter uint32

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
