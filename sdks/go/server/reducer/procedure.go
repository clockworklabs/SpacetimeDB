package reducer

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ProcedureContext provides context to a running procedure.
// Unlike ReducerContext, procedures do not automatically run in a transaction.
// Use WithTx or TryWithTx to access the database.
type ProcedureContext interface {
	Sender() types.Identity
	ConnectionId() types.ConnectionId
	Timestamp() types.Timestamp
	Identity() types.Identity // module identity
	WithTx(fn func())
	TryWithTx(fn func() error) error
	SleepUntil(target types.Timestamp)
	HttpGet(uri string) (statusCode uint16, body []byte, err error)
	NewUuidV7() (types.Uuid, error)
}

// ProcedureFunc is the internal dispatch signature for procedures.
// The args are raw BSATN bytes of the procedure's parameter product type.
// Returns the BSATN-encoded result to write to the sink.
type ProcedureFunc func(ctx ProcedureContext, args []byte) ([]byte, error)
