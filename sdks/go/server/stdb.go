// Package server provides top-level convenience re-exports for SpacetimeDB
// Go module authoring. Import this package to access context types, table
// access constants, and logging.
//
// With the codegen-based approach, registration is handled by generated code
// in stdb_generated.go. Users annotate types and functions with //stdb:
// directives and run `go generate` to produce the registration code.
package server

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/moduledef"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
)

// Table access constants.
const (
	TableAccessPublic  = moduledef.TableAccessPublic
	TableAccessPrivate = moduledef.TableAccessPrivate
)

// Lifecycle constants.
const (
	LifecycleInit               = reducer.LifecycleInit
	LifecycleClientConnected    = reducer.LifecycleClientConnected
	LifecycleClientDisconnected = reducer.LifecycleClientDisconnected
)

// Type aliases for commonly used interfaces.
type (
	ReducerContext       = reducer.ReducerContext
	ViewContext          = reducer.ViewContext
	AnonymousViewContext = reducer.AnonymousViewContext
	ProcedureContext     = reducer.ProcedureContext
)

// NewLogger creates a Logger that writes to the host console.
func NewLogger(target string) log.Logger {
	return log.NewLogger(target)
}
