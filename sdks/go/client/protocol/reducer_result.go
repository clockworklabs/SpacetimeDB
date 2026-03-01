package protocol

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

// ReducerOutcome is a sum type representing the result of a reducer call.
// Tag 0 = Ok, Tag 1 = OkEmpty, Tag 2 = Err, Tag 3 = InternalError.
type ReducerOutcome interface {
	isReducerOutcome()
}

// ReducerOk indicates the reducer succeeded and its transaction committed.
// Contains the return value and transaction update.
type ReducerOk struct {
	RetValue          []byte
	TransactionUpdate *TransactionUpdate
}

func (*ReducerOk) isReducerOutcome() {}

// ReducerOkEmpty indicates the reducer succeeded with zero-length return value
// and zero query set updates. This is a wire-size optimization.
type ReducerOkEmpty struct{}

func (*ReducerOkEmpty) isReducerOutcome() {}

// ReducerErr indicates the reducer returned a structured error
// and its transaction did not commit. The payload is BSATN-encoded.
type ReducerErr struct {
	ErrorBytes []byte
}

func (*ReducerErr) isReducerOutcome() {}

// ReducerInternalError indicates the reducer panicked or failed
// due to a SpacetimeDB internal error.
type ReducerInternalError struct {
	Message string
}

func (*ReducerInternalError) isReducerOutcome() {}

// ProcedureStatus is a sum type representing the result of a procedure call.
// Tag 0 = Returned, Tag 1 = InternalError.
type ProcedureStatus interface {
	isProcedureStatus()
}

// ProcedureReturned indicates the procedure ran and returned a value.
type ProcedureReturned struct {
	Value []byte
}

func (*ProcedureReturned) isProcedureStatus() {}

// ProcedureInternalError indicates the procedure call failed in the host.
type ProcedureInternalError struct {
	Message string
}

func (*ProcedureInternalError) isProcedureStatus() {}

// ReducerResult is the server response to a CallReducer request.
type ReducerResult struct {
	RequestID uint32
	Timestamp types.Timestamp
	Result    ReducerOutcome
}

func (*ReducerResult) serverMessageTag() uint8 { return 6 }

// ProcedureResult is the server response to a CallProcedure request.
// Field order matches the Rust definition: status, timestamp, total_host_execution_duration, request_id.
type ProcedureResult struct {
	Status                     ProcedureStatus
	Timestamp                  types.Timestamp
	TotalHostExecutionDuration types.TimeDuration
	RequestID                  uint32
}

func (*ProcedureResult) serverMessageTag() uint8 { return 7 }
