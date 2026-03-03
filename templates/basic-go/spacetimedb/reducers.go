package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
)

var logger = server.NewLogger("my-module")

//stdb:init
func initReducer(ctx server.ReducerContext) {
	// Called when the module is initially published
}

//stdb:connect
func identityConnected(ctx server.ReducerContext) {
	// Called every time a new client connects
}

//stdb:disconnect
func identityDisconnected(ctx server.ReducerContext) {
	// Called every time a client disconnects
}

//stdb:reducer
func add(ctx server.ReducerContext, name string) {
	PersonTable.Insert(Person{Name: name})
}

//stdb:reducer name=say_hello
func sayHello(ctx server.ReducerContext) {
	iter, err := PersonTable.Scan()
	if err != nil {
		panic(fmt.Sprintf("Scan error: %v", err))
	}
	defer iter.Close()
	for person, ok := iter.Next(); ok; person, ok = iter.Next() {
		logger.Info(fmt.Sprintf("Hello, %s!", person.Name))
	}
	logger.Info("Hello, World!")
}
