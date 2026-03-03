package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

//stdb:reducer
func insertPlayer(_ server.ReducerContext, identity types.Identity, level uint64) {
	player := PlayerTable.Insert(Player{EntityId: 0, Identity: identity})
	PlayerLevelTable.Insert(PlayerLevel{EntityId: player.EntityId, Level: level})
}

//stdb:reducer
func deletePlayer(_ server.ReducerContext, identity types.Identity) {
	player, found, err := PlayerTable.FindByIdentity(identity)
	if err != nil || !found {
		return
	}
	PlayerTable.DeleteByEntityId(player.EntityId)
	PlayerLevelTable.DeleteByEntityId(player.EntityId)
}

//stdb:reducer
func movePlayer(ctx server.ReducerContext, dx int32, dy int32) {
	// Find or create my player.
	myPlayer, found, _ := PlayerTable.FindByIdentity(ctx.Sender())
	if !found {
		myPlayer = PlayerTable.Insert(Player{EntityId: 0, Identity: ctx.Sender()})
	}

	// Find or create my location.
	loc, found, _ := PlayerLocationTable.FindByEntityId(myPlayer.EntityId)
	if found {
		x := loc.X + dx
		y := loc.Y + dy
		PlayerLocationTable.DeleteByEntityId(loc.EntityId)
		PlayerLocationTable.Insert(PlayerLocation{
			EntityId: loc.EntityId,
			Active:   loc.Active,
			X:        x,
			Y:        y,
		})
	} else {
		PlayerLocationTable.Insert(PlayerLocation{
			EntityId: myPlayer.EntityId,
			Active:   true,
			X:        dx,
			Y:        dy,
		})
	}
}
