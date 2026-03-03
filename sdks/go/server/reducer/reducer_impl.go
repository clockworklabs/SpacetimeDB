package reducer

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// NewReducerContext creates a new ReducerContext.
func NewReducerContext(sender types.Identity, connId types.ConnectionId, ts types.Timestamp, moduleIdentity types.Identity) ReducerContext {
	return &reducerContext{
		sender:         sender,
		connectionId:   connId,
		timestamp:      ts,
		moduleIdentity: moduleIdentity,
	}
}

type reducerContext struct {
	sender         types.Identity
	connectionId   types.ConnectionId
	timestamp      types.Timestamp
	moduleIdentity types.Identity
	db             any
}

func (c *reducerContext) Sender() types.Identity         { return c.sender }
func (c *reducerContext) ConnectionId() types.ConnectionId { return c.connectionId }
func (c *reducerContext) Timestamp() types.Timestamp      { return c.timestamp }
func (c *reducerContext) Identity() types.Identity        { return c.moduleIdentity }
func (c *reducerContext) Db() any                         { return c.db }
