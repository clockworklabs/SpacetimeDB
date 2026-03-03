package main

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

//stdb:table name=user access=public
type User struct {
	Identity types.Identity `stdb:"primarykey"`
	Name     *string
	Online   bool
}

//stdb:table name=message access=public
type Message struct {
	Sender types.Identity
	Sent   types.Timestamp
	Text   string
}
