package sys

import "fmt"

// Errno represents a SpacetimeDB host error code.
type Errno uint16

const (
	ErrHostCallFailure         Errno = 1
	ErrNotInTransaction        Errno = 2
	ErrBsatnDecodeError        Errno = 3
	ErrNoSuchTable             Errno = 4
	ErrNoSuchIndex             Errno = 5
	ErrNoSuchIter              Errno = 6
	ErrNoSuchConsoleTimer      Errno = 7
	ErrNoSuchBytes             Errno = 8
	ErrNoSpace                 Errno = 9
	ErrWrongIndexAlgo          Errno = 10
	ErrBufferTooSmall          Errno = 11
	ErrUniqueAlreadyExists     Errno = 12
	ErrScheduleAtDelayTooLong  Errno = 13
	ErrIndexNotUnique          Errno = 14
	ErrNoSuchRow               Errno = 15
	ErrAutoIncOverflow         Errno = 16
	ErrWouldBlockTransaction   Errno = 17
	ErrTransactionNotAnonymous Errno = 18
	ErrTransactionIsReadOnly   Errno = 19
	ErrTransactionIsMut        Errno = 20
	ErrHTTPError               Errno = 21
)

func (e Errno) Error() string {
	switch e {
	case ErrHostCallFailure:
		return "spacetime: host call failure"
	case ErrNotInTransaction:
		return "spacetime: not in transaction"
	case ErrBsatnDecodeError:
		return "spacetime: BSATN decode error"
	case ErrNoSuchTable:
		return "spacetime: no such table"
	case ErrNoSuchIndex:
		return "spacetime: no such index"
	case ErrNoSuchIter:
		return "spacetime: no such iterator"
	case ErrNoSuchConsoleTimer:
		return "spacetime: no such console timer"
	case ErrNoSuchBytes:
		return "spacetime: no such bytes source/sink"
	case ErrNoSpace:
		return "spacetime: no space left in sink"
	case ErrWrongIndexAlgo:
		return "spacetime: wrong index algorithm"
	case ErrBufferTooSmall:
		return "spacetime: buffer too small"
	case ErrUniqueAlreadyExists:
		return "spacetime: unique constraint violation"
	case ErrScheduleAtDelayTooLong:
		return "spacetime: schedule_at delay too long"
	case ErrIndexNotUnique:
		return "spacetime: index is not unique"
	case ErrNoSuchRow:
		return "spacetime: no such row"
	case ErrAutoIncOverflow:
		return "spacetime: auto-increment overflow"
	default:
		return fmt.Sprintf("spacetime: unknown error %d", uint16(e))
	}
}

func errnoFromU16(code uint16) error {
	if code == 0 {
		return nil
	}
	return Errno(code)
}
