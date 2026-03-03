//go:build wasip1

package runtime

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

const (
	callReducerSuccess int32 = 0
	callReducerErr     int32 = 1 // HOST_CALL_FAILURE: user error or panic
)

const (
	callViewRows        int32 = 0 // ViewReturnData::Rows
	callViewHeaderFirst int32 = 2 // ViewReturnData::HeaderFirst
)

// __describe_module__ is called by the host to get the module definition.
//
//go:wasmexport __describe_module__
func wasmDescribeModule(descriptionSink uint32) {
	if describeModuleHandler == nil {
		panic("runtime: no describe module handler registered")
	}
	data := describeModuleHandler()
	_ = sys.WriteBytesToSink(descriptionSink, data)
}

// __call_reducer__ is called by the host to execute a reducer.
//
//go:wasmexport __call_reducer__
func wasmCallReducer(id uint32, sender0, sender1, sender2, sender3, connId0, connId1, timestamp uint64, args uint32, errSink uint32) (retCode int32) {
	// Recover from panics in reducer functions (e.g. Insert/Delete/UpdateBy/DeleteBy
	// panic on host errors, matching Rust SDK behavior where these operations panic).
	defer func() {
		if r := recover(); r != nil {
			writeError(errSink, fmt.Sprintf("%v", r))
			retCode = callReducerErr
		}
	}()

	if callReducerHandler == nil {
		writeError(errSink, "runtime: no call reducer handler registered")
		return callReducerErr
	}

	identity := types.NewIdentityFromU64s(sender0, sender1, sender2, sender3)
	connId := types.NewConnectionIdFromU64s(connId0, connId1)
	ts := types.NewTimestamp(int64(timestamp))
	moduleId := types.NewIdentity(sys.GetIdentity())
	ctx := reducer.NewReducerContext(identity, connId, ts, moduleId)

	argsData, err := sys.ReadBytesSource(args)
	if err != nil {
		writeError(errSink, fmt.Sprintf("failed to read args: %v", err))
		return callReducerErr
	}

	if err := callReducerHandler(id, ctx, argsData); err != nil {
		writeError(errSink, err.Error())
		return callReducerErr
	}

	return callReducerSuccess
}

// __preinit__10_register is called before describe_module.
// Go's init() functions have already run by this point.
//
//go:wasmexport __preinit__10_register
func wasmPreinit() {
	// No-op: Go init() functions run before any wasmexport is called.
}

// __call_view__ is called by the host to execute an authenticated view.
//
//go:wasmexport __call_view__
func wasmCallView(id uint32, sender0, sender1, sender2, sender3 uint64, argsSrc uint32, resultSink uint32) (retCode int32) {
	defer func() {
		if r := recover(); r != nil {
			writeError(resultSink, fmt.Sprintf("%v", r))
			retCode = callReducerErr
		}
	}()

	if callViewHandler == nil {
		writeError(resultSink, "runtime: no call view handler registered")
		return callReducerErr
	}

	identity := types.NewIdentityFromU64s(sender0, sender1, sender2, sender3)

	argsData, err := sys.ReadBytesSource(argsSrc)
	if err != nil {
		writeError(resultSink, fmt.Sprintf("failed to read view args: %v", err))
		return callReducerErr
	}

	result, err := callViewHandler(id, identity, argsData)
	if err != nil {
		writeError(resultSink, err.Error())
		return callReducerErr
	}

	_ = sys.WriteBytesToSink(resultSink, result)
	return callViewHeaderFirst
}

// __call_view_anon__ is called by the host to execute an anonymous view.
//
//go:wasmexport __call_view_anon__
func wasmCallViewAnon(id uint32, argsSrc uint32, resultSink uint32) (retCode int32) {
	defer func() {
		if r := recover(); r != nil {
			writeError(resultSink, fmt.Sprintf("%v", r))
			retCode = callReducerErr
		}
	}()

	if callViewAnonHandler == nil {
		writeError(resultSink, "runtime: no call view anon handler registered")
		return callReducerErr
	}

	argsData, err := sys.ReadBytesSource(argsSrc)
	if err != nil {
		writeError(resultSink, fmt.Sprintf("failed to read view args: %v", err))
		return callReducerErr
	}

	result, err := callViewAnonHandler(id, argsData)
	if err != nil {
		writeError(resultSink, err.Error())
		return callReducerErr
	}

	_ = sys.WriteBytesToSink(resultSink, result)
	return callViewHeaderFirst
}

// __call_procedure__ is called by the host to execute a procedure.
// Procedures always return 0. On error, they panic which becomes a WASM trap.
//
//go:wasmexport __call_procedure__
func wasmCallProcedure(id uint32, sender0, sender1, sender2, sender3, connId0, connId1, timestamp uint64, args uint32, resultSink uint32) int32 {
	if callProcedureHandler == nil {
		panic("runtime: no call procedure handler registered")
	}

	identity := types.NewIdentityFromU64s(sender0, sender1, sender2, sender3)
	connId := types.NewConnectionIdFromU64s(connId0, connId1)
	ts := types.NewTimestamp(int64(timestamp))
	ctx := NewProcedureContext(identity, connId, ts)

	argsData, err := sys.ReadBytesSource(args)
	if err != nil {
		panic(fmt.Sprintf("failed to read procedure args: %v", err))
	}

	result, err := callProcedureHandler(id, ctx, argsData)
	if err != nil {
		panic(fmt.Sprintf("procedure error: %v", err))
	}

	_ = sys.WriteBytesToSink(resultSink, result)
	return callReducerSuccess
}

// writeError writes an error message to the given sink.
func writeError(sink uint32, msg string) {
	_ = sys.WriteBytesToSink(sink, []byte(msg))
}
