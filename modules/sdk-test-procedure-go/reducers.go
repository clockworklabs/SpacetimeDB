package main

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

//stdb:reducer
func scheduleProc(ctx server.ReducerContext) {
	// Schedule the procedure to run in 1s.
	ScheduledProcTableTable.Insert(ScheduledProcTable{
		ScheduledId: 0,
		ScheduledAt: types.ScheduleAtInterval{Value: types.NewTimeDuration(1000 * 1000)}, // 1000ms in micros
		ReducerTs:   ctx.Timestamp(),
		X:           42,
		Y:           24,
	})
}
