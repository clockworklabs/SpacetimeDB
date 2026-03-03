package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
)

//stdb:reducer
func emitTestEvent(_ server.ReducerContext, name string, value uint64) {
	TestEventTable.Insert(TestEvent{Name: name, Value: value})
}

//stdb:reducer
func emitMultipleTestEvents(_ server.ReducerContext) {
	TestEventTable.Insert(TestEvent{Name: "a", Value: 1})
	TestEventTable.Insert(TestEvent{Name: "b", Value: 2})
	TestEventTable.Insert(TestEvent{Name: "c", Value: 3})
}

//stdb:reducer
func noop(_ server.ReducerContext) {}
