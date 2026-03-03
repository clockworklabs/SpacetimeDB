package runtime

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// Handler function types — set by generated code in init().

// DescribeModuleHandler returns the BSATN-encoded module definition.
type DescribeModuleHandler func() []byte

// CallReducerHandler dispatches a reducer call by ID.
type CallReducerHandler func(id uint32, ctx reducer.ReducerContext, args []byte) error

// CallProcedureHandler dispatches a procedure call by ID.
type CallProcedureHandler func(id uint32, ctx reducer.ProcedureContext, args []byte) ([]byte, error)

// CallViewHandler dispatches an authenticated view call by ID.
type CallViewHandler func(id uint32, sender types.Identity, args []byte) ([]byte, error)

// CallViewAnonHandler dispatches an anonymous view call by ID.
type CallViewAnonHandler func(id uint32, args []byte) ([]byte, error)

var (
	describeModuleHandler DescribeModuleHandler
	callReducerHandler    CallReducerHandler
	callProcedureHandler  CallProcedureHandler
	callViewHandler       CallViewHandler
	callViewAnonHandler   CallViewAnonHandler
)

// SetDescribeModuleHandler registers the module description handler.
func SetDescribeModuleHandler(h DescribeModuleHandler) { describeModuleHandler = h }

// SetCallReducerHandler registers the reducer dispatch handler.
func SetCallReducerHandler(h CallReducerHandler) { callReducerHandler = h }

// SetCallProcedureHandler registers the procedure dispatch handler.
func SetCallProcedureHandler(h CallProcedureHandler) { callProcedureHandler = h }

// SetCallViewHandler registers the authenticated view dispatch handler.
func SetCallViewHandler(h CallViewHandler) { callViewHandler = h }

// SetCallViewAnonHandler registers the anonymous view dispatch handler.
func SetCallViewAnonHandler(h CallViewAnonHandler) { callViewAnonHandler = h }
