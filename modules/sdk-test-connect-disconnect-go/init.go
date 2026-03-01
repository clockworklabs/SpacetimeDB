package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// Connected stores the identity of a client that connected.
type Connected struct {
	Identity types.Identity `stdb:"identity"`
}

// Disconnected stores the identity of a client that disconnected.
type Disconnected struct {
	Identity types.Identity `stdb:"identity"`
}

func init() {
	// Register tables
	server.RegisterTable[Connected]("connected", server.TableAccessPublic)
	server.RegisterTable[Disconnected]("disconnected", server.TableAccessPublic)

	// Register lifecycle reducers
	server.RegisterLifecycleReducer(server.LifecycleClientConnected, func(ctx reducer.ReducerContext) {
		runtime.Insert(Connected{Identity: ctx.Sender()})
	})
	server.RegisterLifecycleReducer(server.LifecycleClientDisconnected, func(ctx reducer.ReducerContext) {
		runtime.Insert(Disconnected{Identity: ctx.Sender()})
	})
}
