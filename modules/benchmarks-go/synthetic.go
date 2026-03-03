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

	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/log"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
)

// ---------- schemas ----------

// u32_u64_str schema: (id u32, age u64, name string)

//stdb:table name=unique_0_u32_u64_str access=public
type Unique0U32U64Str struct {
	Id   uint32 `stdb:"primarykey"`
	Age  uint64
	Name string
}

//stdb:table name=no_index_u32_u64_str access=public
type NoIndexU32U64Str struct {
	Id   uint32
	Age  uint64
	Name string
}

//stdb:table name=btree_each_column_u32_u64_str access=public
type BtreeEachColumnU32U64Str struct {
	Id   uint32 `stdb:"index=btree"`
	Age  uint64 `stdb:"index=btree"`
	Name string `stdb:"index=btree"`
}

// u32_u64_u64 schema: (id u32, x u64, y u64)

//stdb:table name=unique_0_u32_u64_u64 access=public
type Unique0U32U64U64 struct {
	Id uint32 `stdb:"primarykey"`
	X  uint64
	Y  uint64
}

//stdb:table name=no_index_u32_u64_u64 access=public
type NoIndexU32U64U64 struct {
	Id uint32
	X  uint64
	Y  uint64
}

//stdb:table name=btree_each_column_u32_u64_u64 access=public
type BtreeEachColumnU32U64U64 struct {
	Id uint32 `stdb:"index=btree"`
	X  uint64 `stdb:"index=btree"`
	Y  uint64 `stdb:"index=btree"`
}

// ---------- logger ----------

var benchLogger log.Logger

func init() {
	benchLogger = log.NewLogger("benchmarks")
}

// ---------- empty ----------

//stdb:reducer name=empty
func empty(_ reducer.ReducerContext) {}

// ---------- insert ----------

//stdb:reducer name=insert_unique_0_u32_u64_str
func insertUnique0U32U64Str(_ reducer.ReducerContext, id uint32, age uint64, name string) {
	Unique0U32U64StrTable.Insert(Unique0U32U64Str{Id: id, Age: age, Name: name})
}

//stdb:reducer name=insert_no_index_u32_u64_str
func insertNoIndexU32U64Str(_ reducer.ReducerContext, id uint32, age uint64, name string) {
	NoIndexU32U64StrTable.Insert(NoIndexU32U64Str{Id: id, Age: age, Name: name})
}

//stdb:reducer name=insert_btree_each_column_u32_u64_str
func insertBtreeEachColumnU32U64Str(_ reducer.ReducerContext, id uint32, age uint64, name string) {
	BtreeEachColumnU32U64StrTable.Insert(BtreeEachColumnU32U64Str{Id: id, Age: age, Name: name})
}

//stdb:reducer name=insert_unique_0_u32_u64_u64
func insertUnique0U32U64U64(_ reducer.ReducerContext, id uint32, x uint64, y uint64) {
	Unique0U32U64U64Table.Insert(Unique0U32U64U64{Id: id, X: x, Y: y})
}

//stdb:reducer name=insert_no_index_u32_u64_u64
func insertNoIndexU32U64U64(_ reducer.ReducerContext, id uint32, x uint64, y uint64) {
	NoIndexU32U64U64Table.Insert(NoIndexU32U64U64{Id: id, X: x, Y: y})
}

//stdb:reducer name=insert_btree_each_column_u32_u64_u64
func insertBtreeEachColumnU32U64U64(_ reducer.ReducerContext, id uint32, x uint64, y uint64) {
	BtreeEachColumnU32U64U64Table.Insert(BtreeEachColumnU32U64U64{Id: id, X: x, Y: y})
}

// ---------- insert bulk ----------

//stdb:reducer name=insert_bulk_unique_0_u32_u64_str
func insertBulkUnique0U32U64Str(_ reducer.ReducerContext, people []Unique0U32U64Str) {
	for _, row := range people {
		Unique0U32U64StrTable.Insert(row)
	}
}

//stdb:reducer name=insert_bulk_no_index_u32_u64_str
func insertBulkNoIndexU32U64Str(_ reducer.ReducerContext, people []NoIndexU32U64Str) {
	for _, row := range people {
		NoIndexU32U64StrTable.Insert(row)
	}
}

//stdb:reducer name=insert_bulk_btree_each_column_u32_u64_str
func insertBulkBtreeEachColumnU32U64Str(_ reducer.ReducerContext, people []BtreeEachColumnU32U64Str) {
	for _, row := range people {
		BtreeEachColumnU32U64StrTable.Insert(row)
	}
}

//stdb:reducer name=insert_bulk_unique_0_u32_u64_u64
func insertBulkUnique0U32U64U64(_ reducer.ReducerContext, locs []Unique0U32U64U64) {
	for _, row := range locs {
		Unique0U32U64U64Table.Insert(row)
	}
}

//stdb:reducer name=insert_bulk_no_index_u32_u64_u64
func insertBulkNoIndexU32U64U64(_ reducer.ReducerContext, locs []NoIndexU32U64U64) {
	for _, row := range locs {
		NoIndexU32U64U64Table.Insert(row)
	}
}

//stdb:reducer name=insert_bulk_btree_each_column_u32_u64_u64
func insertBulkBtreeEachColumnU32U64U64(_ reducer.ReducerContext, locs []BtreeEachColumnU32U64U64) {
	for _, row := range locs {
		BtreeEachColumnU32U64U64Table.Insert(row)
	}
}

// ---------- update ----------

//stdb:reducer name=update_bulk_unique_0_u32_u64_u64
func updateBulkUnique0U32U64U64(_ reducer.ReducerContext, rowCount uint32) {
	iter, err := Unique0U32U64U64Table.Scan()
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
		Unique0U32U64U64Table.UpdateById(Unique0U32U64U64{
			Id: row.Id,
			X:  row.X + 1, // wrapping add
			Y:  row.Y,
		})
	}
	if hit != rowCount {
		panic("not enough rows to perform requested amount of updates")
	}
}

//stdb:reducer name=update_bulk_unique_0_u32_u64_str
func updateBulkUnique0U32U64Str(_ reducer.ReducerContext, rowCount uint32) {
	iter, err := Unique0U32U64StrTable.Scan()
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
		Unique0U32U64StrTable.UpdateById(Unique0U32U64Str{
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

//stdb:reducer name=iterate_unique_0_u32_u64_str
func iterateUnique0U32U64Str(_ reducer.ReducerContext) {
	iter, err := Unique0U32U64StrTable.Scan()
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

//stdb:reducer name=iterate_unique_0_u32_u64_u64
func iterateUnique0U32U64U64(_ reducer.ReducerContext) {
	iter, err := Unique0U32U64U64Table.Scan()
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

//stdb:reducer name=filter_unique_0_u32_u64_str_by_id
func filterUnique0U32U64StrById(_ reducer.ReducerContext, id uint32) {
	row, found, err := Unique0U32U64StrTable.FindById(id)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

//stdb:reducer name=filter_no_index_u32_u64_str_by_id
func filterNoIndexU32U64StrById(_ reducer.ReducerContext, id uint32) {
	iter, err := NoIndexU32U64StrTable.Scan()
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

//stdb:reducer name=filter_btree_each_column_u32_u64_str_by_id
func filterBtreeEachColumnU32U64StrById(_ reducer.ReducerContext, id uint32) {
	iter, err := BtreeEachColumnU32U64StrTable.FilterById(id)
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		_ = row
	}
}

//stdb:reducer name=filter_unique_0_u32_u64_u64_by_id
func filterUnique0U32U64U64ById(_ reducer.ReducerContext, id uint32) {
	row, found, err := Unique0U32U64U64Table.FindById(id)
	if err != nil {
		panic(err)
	}
	if found {
		_ = row
	}
}

//stdb:reducer name=filter_no_index_u32_u64_u64_by_id
func filterNoIndexU32U64U64ById(_ reducer.ReducerContext, id uint32) {
	iter, err := NoIndexU32U64U64Table.Scan()
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

//stdb:reducer name=filter_btree_each_column_u32_u64_u64_by_id
func filterBtreeEachColumnU32U64U64ById(_ reducer.ReducerContext, id uint32) {
	iter, err := BtreeEachColumnU32U64U64Table.FilterById(id)
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		_ = row
	}
}

// ---------- filter by name ----------

//stdb:reducer name=filter_unique_0_u32_u64_str_by_name
func filterUnique0U32U64StrByName(_ reducer.ReducerContext, name string) {
	iter, err := Unique0U32U64StrTable.Scan()
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

//stdb:reducer name=filter_no_index_u32_u64_str_by_name
func filterNoIndexU32U64StrByName(_ reducer.ReducerContext, name string) {
	iter, err := NoIndexU32U64StrTable.Scan()
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

//stdb:reducer name=filter_btree_each_column_u32_u64_str_by_name
func filterBtreeEachColumnU32U64StrByName(_ reducer.ReducerContext, name string) {
	iter, err := BtreeEachColumnU32U64StrTable.FilterByName(name)
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		_ = row
	}
}

// ---------- filter by x ----------

//stdb:reducer name=filter_unique_0_u32_u64_u64_by_x
func filterUnique0U32U64U64ByX(_ reducer.ReducerContext, x uint64) {
	iter, err := Unique0U32U64U64Table.Scan()
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

//stdb:reducer name=filter_no_index_u32_u64_u64_by_x
func filterNoIndexU32U64U64ByX(_ reducer.ReducerContext, x uint64) {
	iter, err := NoIndexU32U64U64Table.Scan()
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

//stdb:reducer name=filter_btree_each_column_u32_u64_u64_by_x
func filterBtreeEachColumnU32U64U64ByX(_ reducer.ReducerContext, x uint64) {
	iter, err := BtreeEachColumnU32U64U64Table.FilterByX(x)
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		_ = row
	}
}

// ---------- filter by y ----------

//stdb:reducer name=filter_unique_0_u32_u64_u64_by_y
func filterUnique0U32U64U64ByY(_ reducer.ReducerContext, y uint64) {
	iter, err := Unique0U32U64U64Table.Scan()
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

//stdb:reducer name=filter_no_index_u32_u64_u64_by_y
func filterNoIndexU32U64U64ByY(_ reducer.ReducerContext, y uint64) {
	iter, err := NoIndexU32U64U64Table.Scan()
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

//stdb:reducer name=filter_btree_each_column_u32_u64_u64_by_y
func filterBtreeEachColumnU32U64U64ByY(_ reducer.ReducerContext, y uint64) {
	iter, err := BtreeEachColumnU32U64U64Table.FilterByY(y)
	if err != nil {
		panic(err)
	}
	defer iter.Close()

	for {
		row, ok := iter.Next()
		if !ok {
			break
		}
		_ = row
	}
}

// ---------- delete ----------

//stdb:reducer name=delete_unique_0_u32_u64_str_by_id
func deleteUnique0U32U64StrById(_ reducer.ReducerContext, id uint32) {
	Unique0U32U64StrTable.DeleteById(id)
}

//stdb:reducer name=delete_unique_0_u32_u64_u64_by_id
func deleteUnique0U32U64U64ById(_ reducer.ReducerContext, id uint32) {
	Unique0U32U64U64Table.DeleteById(id)
}

// ---------- clear table ----------

//stdb:reducer name=clear_table_unique_0_u32_u64_str
func clearTableUnique0U32U64Str(_ reducer.ReducerContext) {
	panic("unimplemented")
}

//stdb:reducer name=clear_table_no_index_u32_u64_str
func clearTableNoIndexU32U64Str(_ reducer.ReducerContext) {
	panic("unimplemented")
}

//stdb:reducer name=clear_table_btree_each_column_u32_u64_str
func clearTableBtreeEachColumnU32U64Str(_ reducer.ReducerContext) {
	panic("unimplemented")
}

//stdb:reducer name=clear_table_unique_0_u32_u64_u64
func clearTableUnique0U32U64U64(_ reducer.ReducerContext) {
	panic("unimplemented")
}

//stdb:reducer name=clear_table_no_index_u32_u64_u64
func clearTableNoIndexU32U64U64(_ reducer.ReducerContext) {
	panic("unimplemented")
}

//stdb:reducer name=clear_table_btree_each_column_u32_u64_u64
func clearTableBtreeEachColumnU32U64U64(_ reducer.ReducerContext) {
	panic("unimplemented")
}

// ---------- count ----------

//stdb:reducer name=count_unique_0_u32_u64_str
func countUnique0U32U64Str(_ reducer.ReducerContext) {
	n, err := Unique0U32U64StrTable.Count()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

//stdb:reducer name=count_no_index_u32_u64_str
func countNoIndexU32U64Str(_ reducer.ReducerContext) {
	n, err := NoIndexU32U64StrTable.Count()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

//stdb:reducer name=count_btree_each_column_u32_u64_str
func countBtreeEachColumnU32U64Str(_ reducer.ReducerContext) {
	n, err := BtreeEachColumnU32U64StrTable.Count()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

//stdb:reducer name=count_unique_0_u32_u64_u64
func countUnique0U32U64U64(_ reducer.ReducerContext) {
	n, err := Unique0U32U64U64Table.Count()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

//stdb:reducer name=count_no_index_u32_u64_u64
func countNoIndexU32U64U64(_ reducer.ReducerContext) {
	n, err := NoIndexU32U64U64Table.Count()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

//stdb:reducer name=count_btree_each_column_u32_u64_u64
func countBtreeEachColumnU32U64U64(_ reducer.ReducerContext) {
	n, err := BtreeEachColumnU32U64U64Table.Count()
	if err != nil {
		panic(err)
	}
	benchLogger.Info(fmt.Sprintf("COUNT: %d", n))
}

// ---------- module-specific ----------

//stdb:reducer name=fn_with_1_args
func fnWith1Args(_ reducer.ReducerContext, _ string) {}

//stdb:reducer name=fn_with_32_args
func fnWith32Args(
	_ reducer.ReducerContext,
	_, _, _, _, _, _, _, _ string,
	_, _, _, _, _, _, _, _ string,
	_, _, _, _, _, _, _, _ string,
	_, _, _, _, _, _, _, _ string,
) {
}

//stdb:reducer name=print_many_things
func printManyThings(_ reducer.ReducerContext, n uint32) {
	for i := uint32(0); i < n; i++ {
		benchLogger.Info("hello again!")
	}
}
