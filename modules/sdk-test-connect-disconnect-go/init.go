package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

//stdb:table name=connected access=public
type Connected struct {
	Identity types.Identity `stdb:"identity"`
}

//stdb:table name=disconnected access=public
type Disconnected struct {
	Identity types.Identity `stdb:"identity"`
}

//stdb:connect
func onConnect(ctx reducer.ReducerContext) {
	ConnectedTable.Insert(Connected{Identity: ctx.Sender()})
}

//stdb:disconnect
func onDisconnect(ctx reducer.ReducerContext) {
	DisconnectedTable.Insert(Disconnected{Identity: ctx.Sender()})
}
