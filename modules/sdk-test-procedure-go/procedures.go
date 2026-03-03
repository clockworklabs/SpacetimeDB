package main

import (
	"bytes"
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

//stdb:procedure
func returnPrimitive(_ server.ProcedureContext, lhs uint32, rhs uint32) uint32 {
	return lhs + rhs
}

//stdb:procedure
func returnStruct(_ server.ProcedureContext, a uint32, b string) ReturnStruct {
	return ReturnStruct{A: a, B: b}
}

//stdb:procedure
func returnEnumA(_ server.ProcedureContext, a uint32) ReturnEnum {
	return ReturnEnumA{Value: a}
}

//stdb:procedure
func returnEnumB(_ server.ProcedureContext, b string) ReturnEnum {
	return ReturnEnumB{Value: b}
}

//stdb:procedure
func willPanic(_ server.ProcedureContext) {
	panic("This procedure is expected to panic")
}

func insertMyTable() {
	MyTableTable.Insert(MyTable{
		Field: ReturnStruct{A: 42, B: "magic"},
	})
}

func assertRowCount(ctx server.ProcedureContext, count uint64) {
	ctx.WithTx(func() {
		n, err := MyTableTable.Count()
		if err != nil {
			panic(fmt.Sprintf("Count error: %v", err))
		}
		if n != count {
			panic(fmt.Sprintf("expected %d rows, got %d", count, n))
		}
	})
}

//stdb:procedure
func insertWithTxCommit(ctx server.ProcedureContext) {
	// Insert a row and commit.
	ctx.WithTx(func() {
		insertMyTable()
	})

	// Assert that there's a row.
	assertRowCount(ctx, 1)
}

//stdb:procedure
func insertWithTxRollback(ctx server.ProcedureContext) {
	_ = ctx.TryWithTx(func() error {
		insertMyTable()
		return fmt.Errorf("rollback")
	})

	// Assert that there's not a row.
	assertRowCount(ctx, 0)
}

//stdb:procedure
func scheduledProc(ctx server.ProcedureContext, data ScheduledProcTable) {
	procedureTs := ctx.Timestamp()
	ctx.WithTx(func() {
		ProcInsertsIntoTable.Insert(ProcInsertsInto{
			ReducerTs:   data.ReducerTs,
			ProcedureTs: procedureTs,
			X:           data.X,
			Y:           data.Y,
		})
	})
}

//stdb:procedure
func readMySchema(ctx server.ProcedureContext) string {
	moduleIdentity := ctx.Identity()
	uri := fmt.Sprintf("http://localhost:3000/v1/database/%s/schema?version=9", moduleIdentity)
	code, body, err := ctx.HttpGet(uri)
	if err != nil {
		// Encode debug info as a single field name so serde_json error shows it
		return fmt.Sprintf(`{"ERR_%v___URI_%s": 0}`, err, uri)
	}
	if len(body) == 0 {
		return fmt.Sprintf(`{"EMPTY_code_%d": 0}`, code)
	}
	if body[0] != '{' && body[0] != '[' {
		return fmt.Sprintf(`{"NOTJSON_code_%d_len_%d_first_%x": 0}`, code, len(body), body[:min(20, len(body))])
	}
	return string(body)
}

//stdb:procedure
func invalidRequest(ctx server.ProcedureContext) string {
	_, body, err := ctx.HttpGet("http://foo.invalid/")
	if err != nil {
		return err.Error()
	}
	panic(fmt.Sprintf("Got result from requesting http://foo.invalid... huh?\n%s", string(body)))
}

// uuidToU128BE converts a Uuid's LE-stored bytes to big-endian u128 for comparison.
func uuidToU128BE(u types.Uuid) [16]byte {
	le := u.Bytes()
	var be [16]byte
	for i := 0; i < 16; i++ {
		be[i] = le[15-i]
	}
	return be
}

//stdb:procedure
func sortedUuidsInsert(ctx server.ProcedureContext) {
	ctx.WithTx(func() {
		for i := 0; i < 1000; i++ {
			uuid, err := ctx.NewUuidV7()
			if err != nil {
				panic(fmt.Sprintf("new uuid: %v", err))
			}
			PkUuidTable.Insert(PkUuid{U: uuid, Data: 0})
		}

		// Verify UUIDs are sorted.
		iter, err := PkUuidTable.Scan()
		if err != nil {
			panic(fmt.Sprintf("Scan error: %v", err))
		}
		defer iter.Close()

		var lastUuid types.Uuid
		for row, ok := iter.Next(); ok; row, ok = iter.Next() {
			if lastUuid != nil {
				lastBE := uuidToU128BE(lastUuid)
				currBE := uuidToU128BE(row.U)
				if bytes.Compare(lastBE[:], currBE[:]) >= 0 {
					panic("UUIDs are not sorted correctly")
				}
			}
			lastUuid = row.U
		}
	})
}
