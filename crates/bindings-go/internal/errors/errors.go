package errors

import (
	spacetimedb "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/types"
)

// Errno wraps the spacetimedb.Errno type
type Errno struct {
	errno *spacetimedb.Errno
}

// NewErrno creates a new Errno
func NewErrno(code uint16) *Errno {
	return &Errno{errno: spacetimedb.NewErrno(code)}
}

// Error implements the error interface
func (e *Errno) Error() string {
	code := e.errno.Code()
	switch code {
	case 0x0001:
		return "no such iterator"
	case 0x0002:
		return "buffer too small"
	case 0x0003:
		return "no such table"
	case 0x0004:
		return "no such index"
	case 0x0005:
		return "wrong index algorithm"
	case 0x0006:
		return "BSATN decode error"
	case 0x0007:
		return "memory exhausted"
	case 0x0008:
		return "out of bounds"
	case 0x0009:
		return "not in transaction"
	case 0x000A:
		return "iterator exhausted"
	default:
		return "unknown error"
	}
}

// ErrToErrno converts an error to an Errno
func ErrToErrno(err error) *Errno {
	if err == nil {
		return NewErrno(0)
	}

	switch e := err.(type) {
	case *Errno:
		return e
	default:
		return NewErrno(0x0001)
	}
}

// RowIterClose closes a row iterator
func RowIterClose(iter spacetimedb.RowIter) {
	// TODO: Implement actual iterator cleanup
}

// Error codes
var (
	ErrNoSuchIter       = NewErrno(0x0001)
	ErrBufferTooSmall   = NewErrno(0x0002)
	ErrNoSuchTable      = NewErrno(0x0003)
	ErrNoSuchIndex      = NewErrno(0x0004)
	ErrWrongIndexAlgo   = NewErrno(0x0005)
	ErrBsatnDecode      = NewErrno(0x0006)
	ErrMemoryExhausted  = NewErrno(0x0007)
	ErrOutOfBounds      = NewErrno(0x0008)
	ErrNotInTransaction = NewErrno(0x0009)
	ErrExhausted        = NewErrno(0x000A)
)
