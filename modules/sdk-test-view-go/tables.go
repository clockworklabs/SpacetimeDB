package main

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

//stdb:table name=player access=public
type Player struct {
	EntityId uint64         `stdb:"primarykey,autoinc"`
	Identity types.Identity `stdb:"unique"`
}

//stdb:table name=player_level access=public
type PlayerLevel struct {
	EntityId uint64 `stdb:"unique"`
	Level    uint64 `stdb:"index=btree"`
}

//stdb:table name=player_location access=private
type PlayerLocation struct {
	EntityId uint64 `stdb:"unique"`
	Active   bool   `stdb:"index=btree"`
	X        int32
	Y        int32
}
