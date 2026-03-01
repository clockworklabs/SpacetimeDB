// STDB module used for benchmarks.
//
// This file is tightly bound to the `benchmarks` crate (`crates/bench`).
//
// The various tables in this file need to remain synced with `crates/bench/src/schemas.rs`.
// Field orders, names, and types should be the same.
//
// We instantiate multiple copies of each table. These should be identical
// aside from indexing strategy. Table names must match the template:
//
//	`{IndexStrategy}{TableName}`, in PascalCase.
//
// The reducers need to remain synced with `crates/bench/src/spacetime_module.rs`.
// Reducer names must match the template:
//
//	`{operation}_{index_strategy}_{table_name}`, in snake_case.
//
// The three index strategies are:
//   - `unique`: a single unique key, declared first in the struct.
//   - `no_index`: no indexes.
//   - `btree_each_column`: one index for each column.
package main

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
)

// ---------- schemas ----------

// u32_u64_str schema: (id u32, age u64, name string)

type Unique0U32U64Str struct {
	Id   uint32 `stdb:"primarykey"`
	Age  uint64
	Name string
}

type NoIndexU32U64Str struct {
	Id   uint32
	Age  uint64
	Name string
}

type BtreeEachColumnU32U64Str struct {
	Id   uint32 `stdb:"index=btree"`
	Age  uint64 `stdb:"index=btree"`
	Name string `stdb:"index=btree"`
}

// u32_u64_u64 schema: (id u32, x u64, y u64)

type Unique0U32U64U64 struct {
	Id uint32 `stdb:"primarykey"`
	X  uint64
	Y  uint64
}

type NoIndexU32U64U64 struct {
	Id uint32
	X  uint64
	Y  uint64
}

type BtreeEachColumnU32U64U64 struct {
	Id uint32 `stdb:"index=btree"`
	X  uint64 `stdb:"index=btree"`
	Y  uint64 `stdb:"index=btree"`
}

// ---------- logger ----------

var benchLogger log.Logger

// ---------- init ----------

func init() {
	benchLogger = log.NewLogger("benchmarks")

	// Table registrations (all public)
	server.RegisterTable[Unique0U32U64Str]("unique_0_u32_u64_str", server.TableAccessPublic)
	server.RegisterTable[NoIndexU32U64Str]("no_index_u32_u64_str", server.TableAccessPublic)
	server.RegisterTable[BtreeEachColumnU32U64Str]("btree_each_column_u32_u64_str", server.TableAccessPublic)
	server.RegisterTable[Unique0U32U64U64]("unique_0_u32_u64_u64", server.TableAccessPublic)
	server.RegisterTable[NoIndexU32U64U64]("no_index_u32_u64_u64", server.TableAccessPublic)
	server.RegisterTable[BtreeEachColumnU32U64U64]("btree_each_column_u32_u64_u64", server.TableAccessPublic)

	// ---------- empty ----------
	server.RegisterReducer("empty", empty)

	// ---------- insert ----------
	server.RegisterReducer("insert_unique_0_u32_u64_str", insertUnique0U32U64Str)
	server.RegisterReducer("insert_no_index_u32_u64_str", insertNoIndexU32U64Str)
	server.RegisterReducer("insert_btree_each_column_u32_u64_str", insertBtreeEachColumnU32U64Str)
	server.RegisterReducer("insert_unique_0_u32_u64_u64", insertUnique0U32U64U64)
	server.RegisterReducer("insert_no_index_u32_u64_u64", insertNoIndexU32U64U64)
	server.RegisterReducer("insert_btree_each_column_u32_u64_u64", insertBtreeEachColumnU32U64U64)

	// ---------- insert bulk ----------
	server.RegisterReducer("insert_bulk_unique_0_u32_u64_str", insertBulkUnique0U32U64Str)
	server.RegisterReducer("insert_bulk_no_index_u32_u64_str", insertBulkNoIndexU32U64Str)
	server.RegisterReducer("insert_bulk_btree_each_column_u32_u64_str", insertBulkBtreeEachColumnU32U64Str)
	server.RegisterReducer("insert_bulk_unique_0_u32_u64_u64", insertBulkUnique0U32U64U64)
	server.RegisterReducer("insert_bulk_no_index_u32_u64_u64", insertBulkNoIndexU32U64U64)
	server.RegisterReducer("insert_bulk_btree_each_column_u32_u64_u64", insertBulkBtreeEachColumnU32U64U64)

	// ---------- update ----------
	server.RegisterReducer("update_bulk_unique_0_u32_u64_u64", updateBulkUnique0U32U64U64)
	server.RegisterReducer("update_bulk_unique_0_u32_u64_str", updateBulkUnique0U32U64Str)

	// ---------- iterate ----------
	server.RegisterReducer("iterate_unique_0_u32_u64_str", iterateUnique0U32U64Str)
	server.RegisterReducer("iterate_unique_0_u32_u64_u64", iterateUnique0U32U64U64)

	// ---------- filter by id ----------
	server.RegisterReducer("filter_unique_0_u32_u64_str_by_id", filterUnique0U32U64StrById)
	server.RegisterReducer("filter_no_index_u32_u64_str_by_id", filterNoIndexU32U64StrById)
	server.RegisterReducer("filter_btree_each_column_u32_u64_str_by_id", filterBtreeEachColumnU32U64StrById)
	server.RegisterReducer("filter_unique_0_u32_u64_u64_by_id", filterUnique0U32U64U64ById)
	server.RegisterReducer("filter_no_index_u32_u64_u64_by_id", filterNoIndexU32U64U64ById)
	server.RegisterReducer("filter_btree_each_column_u32_u64_u64_by_id", filterBtreeEachColumnU32U64U64ById)

	// ---------- filter by name ----------
	server.RegisterReducer("filter_unique_0_u32_u64_str_by_name", filterUnique0U32U64StrByName)
	server.RegisterReducer("filter_no_index_u32_u64_str_by_name", filterNoIndexU32U64StrByName)
	server.RegisterReducer("filter_btree_each_column_u32_u64_str_by_name", filterBtreeEachColumnU32U64StrByName)

	// ---------- filter by x ----------
	server.RegisterReducer("filter_unique_0_u32_u64_u64_by_x", filterUnique0U32U64U64ByX)
	server.RegisterReducer("filter_no_index_u32_u64_u64_by_x", filterNoIndexU32U64U64ByX)
	server.RegisterReducer("filter_btree_each_column_u32_u64_u64_by_x", filterBtreeEachColumnU32U64U64ByX)

	// ---------- filter by y ----------
	server.RegisterReducer("filter_unique_0_u32_u64_u64_by_y", filterUnique0U32U64U64ByY)
	server.RegisterReducer("filter_no_index_u32_u64_u64_by_y", filterNoIndexU32U64U64ByY)
	server.RegisterReducer("filter_btree_each_column_u32_u64_u64_by_y", filterBtreeEachColumnU32U64U64ByY)

	// ---------- delete ----------
	server.RegisterReducer("delete_unique_0_u32_u64_str_by_id", deleteUnique0U32U64StrById)
	server.RegisterReducer("delete_unique_0_u32_u64_u64_by_id", deleteUnique0U32U64U64ById)

	// ---------- clear table ----------
	server.RegisterReducer("clear_table_unique_0_u32_u64_str", clearTableUnique0U32U64Str)
	server.RegisterReducer("clear_table_no_index_u32_u64_str", clearTableNoIndexU32U64Str)
	server.RegisterReducer("clear_table_btree_each_column_u32_u64_str", clearTableBtreeEachColumnU32U64Str)
	server.RegisterReducer("clear_table_unique_0_u32_u64_u64", clearTableUnique0U32U64U64)
	server.RegisterReducer("clear_table_no_index_u32_u64_u64", clearTableNoIndexU32U64U64)
	server.RegisterReducer("clear_table_btree_each_column_u32_u64_u64", clearTableBtreeEachColumnU32U64U64)

	// ---------- count ----------
	server.RegisterReducer("count_unique_0_u32_u64_str", countUnique0U32U64Str)
	server.RegisterReducer("count_no_index_u32_u64_str", countNoIndexU32U64Str)
	server.RegisterReducer("count_btree_each_column_u32_u64_str", countBtreeEachColumnU32U64Str)
	server.RegisterReducer("count_unique_0_u32_u64_u64", countUnique0U32U64U64)
	server.RegisterReducer("count_no_index_u32_u64_u64", countNoIndexU32U64U64)
	server.RegisterReducer("count_btree_each_column_u32_u64_u64", countBtreeEachColumnU32U64U64)

	// ---------- module-specific ----------
	server.RegisterReducer("fn_with_1_args", fnWith1Args)
	server.RegisterReducer("fn_with_32_args", fnWith32Args)
	server.RegisterReducer("print_many_things", printManyThings)
}

// ---------- empty ----------

func empty(_ server.ReducerContext) {}

// ---------- insert ----------

func insertUnique0U32U64Str(_ server.ReducerContext, id uint32, age uint64, name string) {
	runtime.Insert(Unique0U32U64Str{Id: id, Age: age, Name: name})
}

func insertNoIndexU32U64Str(_ server.ReducerContext, id uint32, age uint64, name string) {
	runtime.Insert(NoIndexU32U64Str{Id: id, Age: age, Name: name})
}

func insertBtreeEachColumnU32U64Str(_ server.ReducerContext, id uint32, age uint64, name string) {
	runtime.Insert(BtreeEachColumnU32U64Str{Id: id, Age: age, Name: name})
}

func insertUnique0U32U64U64(_ server.ReducerContext, id uint32, x uint64, y uint64) {
	runtime.Insert(Unique0U32U64U64{Id: id, X: x, Y: y})
}

func insertNoIndexU32U64U64(_ server.ReducerContext, id uint32, x uint64, y uint64) {
	runtime.Insert(NoIndexU32U64U64{Id: id, X: x, Y: y})
}

func insertBtreeEachColumnU32U64U64(_ server.ReducerContext, id uint32, x uint64, y uint64) {
	runtime.Insert(BtreeEachColumnU32U64U64{Id: id, X: x, Y: y})
}

// ---------- insert bulk ----------

func insertBulkUnique0U32U64Str(_ server.ReducerContext, people []Unique0U32U64Str) {
	for _, row := range people {
		runtime.Insert(row)
	}
}

func insertBulkNoIndexU32U64Str(_ server.ReducerContext, people []NoIndexU32U64Str) {
	for _, row := range people {
		runtime.Insert(row)
	}
}

func insertBulkBtreeEachColumnU32U64Str(_ server.ReducerContext, people []BtreeEachColumnU32U64Str) {
	for _, row := range people {
		runtime.Insert(row)
	}
}

func insertBulkUnique0U32U64U64(_ server.ReducerContext, locs []Unique0U32U64U64) {
	for _, row := range locs {
		runtime.Insert(row)
	}
}

func insertBulkNoIndexU32U64U64(_ server.ReducerContext, locs []NoIndexU32U64U64) {
	for _, row := range locs {
		runtime.Insert(row)
	}
}

func insertBulkBtreeEachColumnU32U64U64(_ server.ReducerContext, locs []BtreeEachColumnU32U64U64) {
	for _, row := range locs {
		runtime.Insert(row)
	}
}

// ---------- update ----------

func updateBulkUnique0U32U64U64(_ server.ReducerContext, rowCount uint32) {
	iter, err := runtime.Scan[Unique0U32U64U64]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	var hit uint32
	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if hit >= rowCount {
			break
		}
		hit++
		runtime.UpdateBy[Unique0U32U64U64]("unique_0_u32_u64_u64_id_idx_btree", Unique0U32U64U64{
			Id: row.Id,
			X:  row.X + 1, // wrapping add
			Y:  row.Y,
		})
	}
	if hit != rowCount {
		panic("not enough rows to perform requested amount of updates")
	}
}

func updateBulkUnique0U32U64Str(_ server.ReducerContext, rowCount uint32) {
	iter, err := runtime.Scan[Unique0U32U64Str]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	var hit uint32
	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if hit >= rowCount {
			break
		}
		hit++
		runtime.UpdateBy[Unique0U32U64Str]("unique_0_u32_u64_str_id_idx_btree", Unique0U32U64Str{
			Id:   row.Id,
			Age:  row.Age + 1, // wrapping add
			Name: row.Name,
		})
	}
	if hit != rowCount {
		panic("not enough rows to perform requested amount of updates")
	}
}

// ---------- iterate ----------

func iterateUnique0U32U64Str(_ server.ReducerContext) {
	iter, err := runtime.Scan[Unique0U32U64Str]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		// Access each field to prevent optimization.
		_ = row.Id
		_ = row.Age
		_ = row.Name
	}
}

func iterateUnique0U32U64U64(_ server.ReducerContext) {
	iter, err := runtime.Scan[Unique0U32U64U64]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		// Access each field to prevent optimization.
		_ = row.Id
		_ = row.X
		_ = row.Y
	}
}

// ---------- filter by id ----------

func filterUnique0U32U64StrById(_ server.ReducerContext, id uint32) {
	row, found, err := runtime.FindBy[Unique0U32U64Str, uint32]("unique_0_u32_u64_str_id_idx_btree", id)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

func filterNoIndexU32U64StrById(_ server.ReducerContext, id uint32) {
	iter, err := runtime.Scan[NoIndexU32U64Str]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.Id == id {
			_ = row
		}
	}
}

func filterBtreeEachColumnU32U64StrById(_ server.ReducerContext, id uint32) {
	row, found, err := runtime.FindBy[BtreeEachColumnU32U64Str, uint32]("btree_each_column_u32_u64_str_id_idx_btree", id)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

func filterUnique0U32U64U64ById(_ server.ReducerContext, id uint32) {
	row, found, err := runtime.FindBy[Unique0U32U64U64, uint32]("unique_0_u32_u64_u64_id_idx_btree", id)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

func filterNoIndexU32U64U64ById(_ server.ReducerContext, id uint32) {
	iter, err := runtime.Scan[NoIndexU32U64U64]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.Id == id {
			_ = row
		}
	}
}

func filterBtreeEachColumnU32U64U64ById(_ server.ReducerContext, id uint32) {
	row, found, err := runtime.FindBy[BtreeEachColumnU32U64U64, uint32]("btree_each_column_u32_u64_u64_id_idx_btree", id)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

// ---------- filter by name ----------

func filterUnique0U32U64StrByName(_ server.ReducerContext, name string) {
	iter, err := runtime.Scan[Unique0U32U64Str]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.Name == name {
			_ = row
		}
	}
}

func filterNoIndexU32U64StrByName(_ server.ReducerContext, name string) {
	iter, err := runtime.Scan[NoIndexU32U64Str]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.Name == name {
			_ = row
		}
	}
}

func filterBtreeEachColumnU32U64StrByName(_ server.ReducerContext, name string) {
	row, found, err := runtime.FindBy[BtreeEachColumnU32U64Str, string]("btree_each_column_u32_u64_str_name_idx_btree", name)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

// ---------- filter by x ----------

func filterUnique0U32U64U64ByX(_ server.ReducerContext, x uint64) {
	iter, err := runtime.Scan[Unique0U32U64U64]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.X == x {
			_ = row
		}
	}
}

func filterNoIndexU32U64U64ByX(_ server.ReducerContext, x uint64) {
	iter, err := runtime.Scan[NoIndexU32U64U64]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.X == x {
			_ = row
		}
	}
}

func filterBtreeEachColumnU32U64U64ByX(_ server.ReducerContext, x uint64) {
	row, found, err := runtime.FindBy[BtreeEachColumnU32U64U64, uint64]("btree_each_column_u32_u64_u64_x_idx_btree", x)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

// ---------- filter by y ----------

func filterUnique0U32U64U64ByY(_ server.ReducerContext, y uint64) {
	iter, err := runtime.Scan[Unique0U32U64U64]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.Y == y {
			_ = row
		}
	}
}

func filterNoIndexU32U64U64ByY(_ server.ReducerContext, y uint64) {
	iter, err := runtime.Scan[NoIndexU32U64U64]()
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		if row.Y == y {
			_ = row
		}
	}
}

func filterBtreeEachColumnU32U64U64ByY(_ server.ReducerContext, y uint64) {
	row, found, err := runtime.FindBy[BtreeEachColumnU32U64U64, uint64]("btree_each_column_u32_u64_u64_y_idx_btree", y)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

// ---------- delete ----------

func deleteUnique0U32U64StrById(_ server.ReducerContext, id uint32) {
	runtime.DeleteBy[Unique0U32U64Str, uint32]("unique_0_u32_u64_str_id_idx_btree", id)
}

func deleteUnique0U32U64U64ById(_ server.ReducerContext, id uint32) {
	runtime.DeleteBy[Unique0U32U64U64, uint32]("unique_0_u32_u64_u64_id_idx_btree", id)
}

// ---------- clear table ----------

func clearTableUnique0U32U64Str(_ server.ReducerContext) {
	panic("unimplemented")
}

func clearTableNoIndexU32U64Str(_ server.ReducerContext) {
	panic("unimplemented")
}

func clearTableBtreeEachColumnU32U64Str(_ server.ReducerContext) {
	panic("unimplemented")
}

func clearTableUnique0U32U64U64(_ server.ReducerContext) {
	panic("unimplemented")
}

func clearTableNoIndexU32U64U64(_ server.ReducerContext) {
	panic("unimplemented")
}

func clearTableBtreeEachColumnU32U64U64(_ server.ReducerContext) {
	panic("unimplemented")
}

// ---------- count ----------

func countUnique0U32U64Str(_ server.ReducerContext) {
	n, err := runtime.Count[Unique0U32U64Str]()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

func countNoIndexU32U64Str(_ server.ReducerContext) {
	n, err := runtime.Count[NoIndexU32U64Str]()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

func countBtreeEachColumnU32U64Str(_ server.ReducerContext) {
	n, err := runtime.Count[BtreeEachColumnU32U64Str]()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

func countUnique0U32U64U64(_ server.ReducerContext) {
	n, err := runtime.Count[Unique0U32U64U64]()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

func countNoIndexU32U64U64(_ server.ReducerContext) {
	n, err := runtime.Count[NoIndexU32U64U64]()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

func countBtreeEachColumnU32U64U64(_ server.ReducerContext) {
	n, err := runtime.Count[BtreeEachColumnU32U64U64]()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

// ---------- module-specific ----------

func fnWith1Args(_ server.ReducerContext, _ string) {}

func fnWith32Args(
	_ server.ReducerContext,
	_, _, _, _, _, _, _, _ string,
	_, _, _, _, _, _, _, _ string,
	_, _, _, _, _, _, _, _ string,
	_, _, _, _, _, _, _, _ string,
) {
}

func printManyThings(_ server.ReducerContext, n uint32) {
	for i := uint32(0); i < n; i++ {
		benchLogger.Info("hello again!")
	}
}
