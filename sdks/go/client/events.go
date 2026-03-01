package client

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

// EventContext is passed to table insert/delete callbacks.
type EventContext struct {
	Identity     types.Identity
	ConnectionID types.ConnectionId
	Timestamp    types.Timestamp
	Conn         DbConnection
}

// ReducerEventContext is passed to reducer result callbacks.
type ReducerEventContext struct {
	EventContext
	ReducerName string
	Status      string
	ErrMessage  string
}

// ErrorContext is passed to error callbacks.
type ErrorContext struct {
	Err error
}
