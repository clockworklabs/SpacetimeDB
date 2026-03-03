package main

import (
	"fmt"
	"time"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

//stdb:procedure
func sleepOneSecond(ctx server.ProcedureContext) {
	prevTime := ctx.Timestamp()
	target := types.NewTimestamp(prevTime.Microseconds() + int64(time.Second/time.Microsecond))
	ctx.SleepUntil(target)
	newTime := ctx.Timestamp()
	actualDelta := newTime.Microseconds() - prevTime.Microseconds()
	logger.Info(fmt.Sprintf("Slept from %v to %v, a total of %d microseconds", prevTime, newTime, actualDelta))
}

//stdb:procedure
func returnValue(_ server.ProcedureContext, foo uint64) Baz {
	return Baz{Field: fmt.Sprintf("%d", foo)}
}

//stdb:procedure name=with_tx
func withTx(ctx server.ProcedureContext) {
	ctx.WithTx(func() {
		sayHelloInTx()
	})
}

// sayHelloInTx is the same logic as sayHello but callable within a procedure transaction.
func sayHelloInTx() {
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

//stdb:procedure name=get_my_schema_via_http
func getMySchemaViaHttp(ctx server.ProcedureContext) string {
	moduleIdentity := ctx.Identity()
	uri := fmt.Sprintf("http://localhost:3000/v1/database/%s/schema?version=9", moduleIdentity)
	_, body, err := ctx.HttpGet(uri)
	if err != nil {
		panic(fmt.Sprintf("HTTP error: %v", err))
	}
	return string(body)
}
