package main

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

type PlayerAndLevel struct {
	EntityId uint64
	Identity types.Identity
	Level    uint64
}
