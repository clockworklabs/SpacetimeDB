package main

//stdb:table name=location access=private index=location_coordinates_idx_btree:2,3,4
type Location struct {
	Id        uint64 `stdb:"primarykey"`
	Chunk     uint64 `stdb:"index=btree"`
	X         int32  `stdb:"index=btree"`
	Z         int32
	Dimension uint32
}
