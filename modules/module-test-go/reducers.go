package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

var logger = log.NewLogger("module-test-go")

//stdb:init
func initReducer(ctx server.ReducerContext) {
	RepeatingTestArgTable.Insert(RepeatingTestArg{
		ScheduledId: 0,
		ScheduledAt: types.ScheduleAtInterval{Value: types.NewTimeDuration(1000 * 1000)}, // 1000ms in micros
		PrevTime:    ctx.Timestamp(),
	})
}

//stdb:reducer
func repeatingTest(ctx server.ReducerContext, arg RepeatingTestArg) {
	deltaTime := ctx.Timestamp().Microseconds() - arg.PrevTime.Microseconds()
	logger.Trace(fmt.Sprintf("Timestamp: %v, Delta time: %d", ctx.Timestamp(), deltaTime))
}

//stdb:reducer
func add(ctx server.ReducerContext, name string, age uint8) {
	_ = ctx
	PersonTable.Insert(Person{Id: 0, Name: name, Age: age})
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

//stdb:reducer name=list_over_age
func listOverAge(ctx server.ReducerContext, age uint8) {
	_ = ctx
	iter, err := PersonTable.Scan()
	if err != nil {
		panic(fmt.Sprintf("Scan error: %v", err))
	}
	defer iter.Close()
	for person, ok := iter.Next(); ok; person, ok = iter.Next() {
		if person.Age >= age {
			logger.Info(fmt.Sprintf("%s has age %d >= %d", person.Name, person.Age, age))
		}
	}
}

//stdb:reducer name=log_module_identity
func logModuleIdentity(ctx server.ReducerContext) {
	logger.Info(fmt.Sprintf("Module identity: %v", ctx.Identity()))
}

//stdb:reducer
func test(ctx server.ReducerContext, arg TestAlias, arg2 TestB, arg3 TestC, arg4 TestF) error {
	logger.Info("BEGIN")
	logger.Info(fmt.Sprintf("sender: %v", ctx.Sender()))
	logger.Info(fmt.Sprintf("timestamp: %v", ctx.Timestamp()))
	logger.Info(fmt.Sprintf(`bar: "%s"`, arg2.Foo))

	switch arg3 {
	case TestCFoo:
		logger.Info("Foo")
	case TestCBar:
		logger.Info("Bar")
	}

	switch v := arg4.(type) {
	case TestFFoo:
		logger.Info("Foo")
	case TestFBar:
		logger.Info("Bar")
	case TestFBaz:
		logger.Info(v.Value)
	}

	for i := uint32(0); i < 1000; i++ {
		TestATable.Insert(TestA{
			X: i + arg.X,
			Y: i + arg.Y,
			Z: "Yo",
		})
	}

	rowCountBeforeDelete, err := TestATable.Count()
	if err != nil {
		return fmt.Errorf("Count error: %w", err)
	}
	logger.Info(fmt.Sprintf("Row count before delete: %d", rowCountBeforeDelete))

	var numDeleted uint32
	for row := uint32(5); row < 10; row++ {
		numDeleted += TestATable.DeleteByX(row)
	}

	rowCountAfterDelete, err := TestATable.Count()
	if err != nil {
		return fmt.Errorf("Count error: %w", err)
	}

	if rowCountBeforeDelete != rowCountAfterDelete+uint64(numDeleted) {
		logger.Error(fmt.Sprintf(
			"Started with %d rows, deleted %d, and wound up with %d rows... huh?",
			rowCountBeforeDelete, numDeleted, rowCountAfterDelete,
		))
	}

	inserted := TestETable.Insert(TestE{Id: 0, Name: "Tyler"})
	logger.Info(fmt.Sprintf(`Inserted: TestE { id: %d, name: "%s" }`, inserted.Id, inserted.Name))

	logger.Info(fmt.Sprintf("Row count after delete: %d", rowCountAfterDelete))

	otherRowCount, err := TestATable.Count()
	if err != nil {
		return fmt.Errorf("Count error: %w", err)
	}
	logger.Info(fmt.Sprintf("Row count filtered by condition: %d", otherRowCount))

	logger.Info("MultiColumn")

	for i := int64(0); i < 1000; i++ {
		PointsTable.Insert(Point{
			X: i + int64(arg.X),
			Y: i + int64(arg.Y),
		})
	}

	pointIter, err := PointsTable.Scan()
	if err != nil {
		return fmt.Errorf("Scan error: %w", err)
	}
	defer pointIter.Close()
	multiRowCount := 0
	for point, ok := pointIter.Next(); ok; point, ok = pointIter.Next() {
		if point.X >= 0 && point.Y <= 200 {
			multiRowCount++
		}
	}
	logger.Info(fmt.Sprintf("Row count filtered by multi-column condition: %d", multiRowCount))

	logger.Info("END")
	return nil
}

//stdb:reducer name=add_player
func addPlayer(ctx server.ReducerContext, name string) error {
	_ = ctx
	// Insert always creates a new row since id is auto_inc with value 0.
	inserted := TestETable.Insert(TestE{Id: 0, Name: name})

	// Update the same row (no-op, but verifies UpdateBy works).
	TestETable.UpdateById(inserted)

	return nil
}

//stdb:reducer name=delete_player
func deletePlayer(ctx server.ReducerContext, id uint64) error {
	_ = ctx
	deleted := TestETable.DeleteById(id)
	if deleted == 0 {
		return fmt.Errorf("No TestE row with id %d", id)
	}
	return nil
}

//stdb:reducer name=delete_players_by_name
func deletePlayersByName(ctx server.ReducerContext, name string) error {
	_ = ctx
	deleted := TestETable.DeleteByName(name)
	if deleted == 0 {
		return fmt.Errorf("No TestE row with name %q", name)
	}
	logger.Info(fmt.Sprintf("Deleted %d player(s) with name %q", deleted, name))
	return nil
}

//stdb:connect
func clientConnected(_ server.ReducerContext) {}

//stdb:reducer name=add_private
func addPrivate(ctx server.ReducerContext, name string) {
	_ = ctx
	PrivateTableTable.Insert(PrivateTable{Name: name})
}

//stdb:reducer name=query_private
func queryPrivate(ctx server.ReducerContext) {
	_ = ctx
	iter, err := PrivateTableTable.Scan()
	if err != nil {
		panic(fmt.Sprintf("Scan error: %v", err))
	}
	defer iter.Close()
	for person, ok := iter.Next(); ok; person, ok = iter.Next() {
		logger.Info(fmt.Sprintf("Private, %s!", person.Name))
	}
	logger.Info("Private, World!")
}

//stdb:reducer name=test_btree_index_args
func testBtreeIndexArgs(ctx server.ReducerContext) {
	_ = ctx
	// This reducer tests that various index operations compile and work.

	// Single-column string index on test_e.name
	str := "String"
	_, _ = TestETable.FilterByName(str)
	_, _ = TestETable.FilterByName("str")

	TestETable.DeleteByName(str)
	TestETable.DeleteByName("str")

	// Multi-column index on points (x, y)
	_, _ = PointsTable.FilterByX(int64(0))
	_, _ = PointsTable.FilterByXAndY(int64(0), int64(1))
}

//stdb:reducer name=assert_caller_identity_is_module_identity
func assertCallerIdentityIsModuleIdentity(ctx server.ReducerContext) {
	caller := ctx.Sender()
	owner := ctx.Identity()
	if caller != owner {
		panic(fmt.Sprintf("Caller %v is not the owner %v", caller, owner))
	}
	logger.Info(fmt.Sprintf("Called by the owner %v", owner))
}
