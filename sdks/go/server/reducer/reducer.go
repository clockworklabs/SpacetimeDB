package reducer

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ReducerContext provides context to a running reducer.
type ReducerContext interface {
	Sender() types.Identity
	ConnectionId() types.ConnectionId
	Timestamp() types.Timestamp
	Identity() types.Identity // Module identity (owner)
	Db() any
}

// ReducerFunc is the internal dispatch signature for reducers.
// The args are raw BSATN bytes of the reducer's parameter product type.
type ReducerFunc func(ctx ReducerContext, args []byte) error
