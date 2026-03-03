package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
)

//stdb:view name=my_player public=true
func myPlayerView(ctx server.ViewContext) *Player {
	player, found, err := PlayerTable.FindByIdentity(ctx.Sender())
	if err != nil || !found {
		return nil
	}
	return &player
}
