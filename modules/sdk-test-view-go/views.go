package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
)

//stdb:view public=true
func myPlayer(ctx server.ViewContext) *Player {
	player, found, _ := PlayerTable.FindByIdentity(ctx.Sender())
	if !found {
		return nil
	}
	return &player
}

//stdb:view public=true
func myPlayerAndLevel(ctx server.ViewContext) *PlayerAndLevel {
	player, found, _ := PlayerTable.FindByIdentity(ctx.Sender())
	if !found {
		return nil
	}
	level, found, _ := PlayerLevelTable.FindByEntityId(player.EntityId)
	if !found {
		return nil
	}
	return &PlayerAndLevel{
		EntityId: player.EntityId,
		Identity: player.Identity,
		Level:    level.Level,
	}
}

//stdb:view public=true
func playersAtLevel0(_ server.AnonymousViewContext) []Player {
	iter, err := PlayerLevelTable.FilterByLevel(0)
	if err != nil {
		return nil
	}
	var players []Player
	for {
		pl, ok := iter.Next()
		if !ok {
			break
		}
		player, found, _ := PlayerTable.FindByEntityId(pl.EntityId)
		if found {
			players = append(players, player)
		}
	}
	return players
}

//stdb:view public=true
func nearbyPlayers(ctx server.ViewContext) []PlayerLocation {
	myPlayerRow, found, _ := PlayerTable.FindByIdentity(ctx.Sender())
	if !found {
		return nil
	}
	myLoc, found, _ := PlayerLocationTable.FindByEntityId(myPlayerRow.EntityId)
	if !found {
		return nil
	}

	iter, err := PlayerLocationTable.FilterByActive(true)
	if err != nil {
		return nil
	}
	var result []PlayerLocation
	for {
		loc, ok := iter.Next()
		if !ok {
			break
		}
		if loc.EntityId == myLoc.EntityId {
			continue
		}
		dx := loc.X - myLoc.X
		dy := loc.Y - myLoc.Y
		if dx < 0 {
			dx = -dx
		}
		if dy < 0 {
			dy = -dy
		}
		if dx < 5 && dy < 5 {
			result = append(result, loc)
		}
	}
	return result
}
