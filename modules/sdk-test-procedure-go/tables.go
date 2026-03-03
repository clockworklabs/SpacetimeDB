package main

import "github.com/clockworklabs/SpacetimeDB/sdks/go/types"

//stdb:table name=my_table access=public
type MyTable struct {
	Field ReturnStruct
}

//stdb:table name=scheduled_proc_table access=private
//stdb:schedule table=scheduled_proc_table function=scheduled_proc
type ScheduledProcTable struct {
	ScheduledId uint64         `stdb:"primarykey,autoinc"`
	ScheduledAt types.ScheduleAt
	ReducerTs   types.Timestamp
	X           uint8
	Y           uint8
}

//stdb:table name=proc_inserts_into access=public
type ProcInsertsInto struct {
	ReducerTs   types.Timestamp
	ProcedureTs types.Timestamp
	X           uint8
	Y           uint8
}

//stdb:table name=pk_uuid access=public
type PkUuid struct {
	U    types.Uuid
	Data uint8
}
