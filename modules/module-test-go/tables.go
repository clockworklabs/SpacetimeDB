package main

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

// Person table — PK auto_inc id, btree on age, public.
//stdb:table name=person access=public index=person_age_idx_btree:2
type Person struct {
	Id   uint32 `stdb:"primarykey,autoinc"`
	Name string
	Age  uint8
}

// RemoveTable — default (no feature flag) version.
//stdb:table name=table_to_remove access=private
type RemoveTable struct {
	Id uint32
}

// TestA table — btree on x, private.
//stdb:table name=test_a access=private
type TestA struct {
	X uint32 `stdb:"index=btree"`
	Y uint32
	Z string
}

// TestD table — optional TestC field, public.
//stdb:table name=test_d access=public
type TestD struct {
	TestC *TestC
}

// TestE table — PK auto_inc id, btree on name, private.
//stdb:table name=test_e access=private
type TestE struct {
	Id   uint64 `stdb:"primarykey,autoinc"`
	Name string `stdb:"index=btree"`
}

// TestFoobar table — field of sum type Foobar, public.
//stdb:table name=test_f access=public
type TestFoobar struct {
	Field Foobar
}

// PrivateTable — private table with a name field.
//stdb:table name=private_table access=private
type PrivateTable struct {
	Name string
}

// Point table — multi-column btree on (x, y), private.
//stdb:table name=points access=private index=points_multi_column_index_idx_btree:0,1
type Point struct {
	X int64
	Y int64
}

// PkMultiIdentity — PK on id, unique auto_inc on other, private.
//stdb:table name=pk_multi_identity access=private
type PkMultiIdentity struct {
	Id    uint32 `stdb:"primarykey"`
	Other uint32 `stdb:"unique,autoinc"`
}

// RepeatingTestArg — scheduled table, PK auto_inc scheduled_id.
//stdb:table name=repeating_test_arg access=private
//stdb:schedule table=repeating_test_arg function=repeating_test
type RepeatingTestArg struct {
	ScheduledId uint64         `stdb:"primarykey,autoinc"`
	ScheduledAt types.ScheduleAt
	PrevTime    types.Timestamp
}

// HasSpecialStuff — table with Identity and ConnectionId fields.
//stdb:table name=has_special_stuff access=private
type HasSpecialStuff struct {
	Identity     types.Identity
	ConnectionId types.ConnectionId
}

// Player — PK on identity, auto_inc unique player_id, unique name, public.
// Used by both the "player" and "logged_out_player" tables.
//stdb:table name=player access=public
//stdb:table name=logged_out_player access=public
type Player struct {
	Identity types.Identity `stdb:"primarykey"`
	PlayerId uint64         `stdb:"autoinc,unique"`
	Name     string         `stdb:"unique"`
}
