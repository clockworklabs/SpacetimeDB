//! STDB module used for benchmarks.
//!
//! This file is tightly bound to the `benchmarks` crate (`crates/bench`).
//!
//! The various tables in this file need to remain synced with `crates/bench/src/schemas.rs`
//! Field orders, names, and types should be the same.
//!
//! We instantiate multiple copies of each table. These should be identical
//! aside from indexing strategy. Table names must match the template:
//!
//! `{IndexStrategy}{TableName}`, in PascalCase.
//!
//! The reducers need to remain synced with `crates/bench/src/spacetime_module.rs`
//! Reducer names must match the template:
//!
//! `{operation}_{index_strategy}_{table_name}`, in snake_case.
//!
//! The three index strategies are:
//! - `unique`: a single unique key, declared first in the struct.
//! - `no_index`: no indexes.
//! - `btree_each_column`: one index for each column.
//!
//! Obviously more could be added...
#![allow(non_camel_case_types)]
#![allow(clippy::too_many_arguments)]
use spacetimedb::println;
use std::hint::black_box;

// ---------- schemas ----------

#[spacetimedb::table(name = unique_0_u32_u64_str)]
pub struct unique_0_u32_u64_str {
    #[unique]
    id: u32,
    age: u64,
    name: String,
}

#[spacetimedb::table(name = no_index_u32_u64_str)]
pub struct no_index_u32_u64_str {
    id: u32,
    age: u64,
    name: String,
}

#[spacetimedb::table(name = btree_each_column_u32_u64_str)]
pub struct btree_each_column_u32_u64_str {
    #[index(btree)]
    id: u32,
    #[index(btree)]
    age: u64,
    #[index(btree)]
    name: String,
}

#[spacetimedb::table(name = unique_0_u32_u64_u64)]
pub struct unique_0_u32_u64_u64 {
    #[unique]
    id: u32,
    x: u64,
    y: u64,
}

#[spacetimedb::table(name = no_index_u32_u64_u64)]
pub struct no_index_u32_u64_u64 {
    id: u32,
    x: u64,
    y: u64,
}

#[spacetimedb::table(name = btree_each_column_u32_u64_u64)]
pub struct btree_each_column_u32_u64_u64 {
    #[index(btree)]
    id: u32,
    #[index(btree)]
    x: u64,
    #[index(btree)]
    y: u64,
}

// ---------- empty ----------

#[spacetimedb::reducer]
pub fn empty() {}

// ---------- insert ----------
#[spacetimedb::reducer]
pub fn insert_unique_0_u32_u64_str(id: u32, age: u64, name: String) {
    unique_0_u32_u64_str::insert(unique_0_u32_u64_str { id, name, age }).unwrap();
}

#[spacetimedb::reducer]
pub fn insert_no_index_u32_u64_str(id: u32, age: u64, name: String) {
    no_index_u32_u64_str::insert(no_index_u32_u64_str { id, name, age });
}

#[spacetimedb::reducer]
pub fn insert_btree_each_column_u32_u64_str(id: u32, age: u64, name: String) {
    btree_each_column_u32_u64_str::insert(btree_each_column_u32_u64_str { id, name, age });
}

#[spacetimedb::reducer]
pub fn insert_unique_0_u32_u64_u64(id: u32, x: u64, y: u64) {
    unique_0_u32_u64_u64::insert(unique_0_u32_u64_u64 { id, x, y }).unwrap();
}

#[spacetimedb::reducer]
pub fn insert_no_index_u32_u64_u64(id: u32, x: u64, y: u64) {
    no_index_u32_u64_u64::insert(no_index_u32_u64_u64 { id, x, y });
}

#[spacetimedb::reducer]
pub fn insert_btree_each_column_u32_u64_u64(id: u32, x: u64, y: u64) {
    btree_each_column_u32_u64_u64::insert(btree_each_column_u32_u64_u64 { id, x, y });
}

// ---------- insert bulk ----------

#[spacetimedb::reducer]
pub fn insert_bulk_unique_0_u32_u64_u64(locs: Vec<unique_0_u32_u64_u64>) {
    for loc in locs {
        unique_0_u32_u64_u64::insert(loc).unwrap();
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_no_index_u32_u64_u64(locs: Vec<no_index_u32_u64_u64>) {
    for loc in locs {
        no_index_u32_u64_u64::insert(loc);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_btree_each_column_u32_u64_u64(locs: Vec<btree_each_column_u32_u64_u64>) {
    for loc in locs {
        btree_each_column_u32_u64_u64::insert(loc);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_unique_0_u32_u64_str(people: Vec<unique_0_u32_u64_str>) {
    for u32_u64_str in people {
        unique_0_u32_u64_str::insert(u32_u64_str).unwrap();
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_no_index_u32_u64_str(people: Vec<no_index_u32_u64_str>) {
    for u32_u64_str in people {
        no_index_u32_u64_str::insert(u32_u64_str);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_btree_each_column_u32_u64_str(people: Vec<btree_each_column_u32_u64_str>) {
    for u32_u64_str in people {
        btree_each_column_u32_u64_str::insert(u32_u64_str);
    }
}

// ---------- update ----------

#[spacetimedb::reducer]
pub fn update_bulk_unique_0_u32_u64_u64(row_count: u32) {
    let mut hit: u32 = 0;
    for loc in unique_0_u32_u64_u64::iter().take(row_count as usize) {
        hit += 1;
        assert!(
            unique_0_u32_u64_u64::update_by_id(
                &loc.id,
                unique_0_u32_u64_u64 {
                    id: loc.id,
                    x: loc.x.wrapping_add(1),
                    y: loc.y,
                },
            ),
            "failed to update u32_u64_u64"
        );
    }
    assert_eq!(hit, row_count, "not enough rows to perform requested amount of updates");
}

#[spacetimedb::reducer]
pub fn update_bulk_unique_0_u32_u64_str(row_count: u32) {
    let mut hit: u32 = 0;
    for u32_u64_str in unique_0_u32_u64_str::iter().take(row_count as usize) {
        hit += 1;
        assert!(
            unique_0_u32_u64_str::update_by_id(
                &u32_u64_str.id,
                unique_0_u32_u64_str {
                    id: u32_u64_str.id,
                    name: u32_u64_str.name,
                    age: u32_u64_str.age.wrapping_add(1),
                },
            ),
            "failed to update u32_u64_str"
        );
    }
    assert_eq!(hit, row_count, "not enough rows to perform requested amount of updates");
}

// ---------- iterate ----------

#[spacetimedb::reducer]
pub fn iterate_unique_0_u32_u64_str() {
    for u32_u64_str in unique_0_u32_u64_str::iter() {
        black_box(u32_u64_str);
    }
}
#[spacetimedb::reducer]
pub fn iterate_unique_0_u32_u64_u64() {
    for u32_u64_u64 in unique_0_u32_u64_u64::iter() {
        black_box(u32_u64_u64);
    }
}

// ---------- filtering ----------

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_str_by_id(id: u32) {
    if let Some(p) = unique_0_u32_u64_str::filter_by_id(&id) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_str_by_id(id: u32) {
    for p in no_index_u32_u64_str::filter_by_id(&id) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_str_by_id(id: u32) {
    for p in btree_each_column_u32_u64_str::filter_by_id(&id) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_str_by_name(name: String) {
    for p in unique_0_u32_u64_str::filter_by_name(&name) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_str_by_name(name: String) {
    for p in no_index_u32_u64_str::filter_by_name(&name) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_str_by_name(name: String) {
    for p in btree_each_column_u32_u64_str::filter_by_name(&name) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_u64_by_id(id: u32) {
    if let Some(loc) = unique_0_u32_u64_u64::filter_by_id(&id) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_u64_by_id(id: u32) {
    for loc in no_index_u32_u64_u64::filter_by_id(&id) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_u64_by_id(id: u32) {
    for loc in btree_each_column_u32_u64_u64::filter_by_id(&id) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_u64_by_x(x: u64) {
    for loc in unique_0_u32_u64_u64::filter_by_x(&x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_u64_by_x(x: u64) {
    for loc in no_index_u32_u64_u64::filter_by_x(&x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_u64_by_x(x: u64) {
    for loc in btree_each_column_u32_u64_u64::filter_by_x(&x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_u64_by_y(x: u64) {
    for loc in unique_0_u32_u64_u64::filter_by_y(&x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_u64_by_y(x: u64) {
    for loc in no_index_u32_u64_u64::filter_by_y(&x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_u64_by_y(x: u64) {
    for loc in btree_each_column_u32_u64_u64::filter_by_y(&x) {
        black_box(loc);
    }
}

// ---------- delete ----------

// FIXME: current nonunique delete interface is UNUSABLE!!!!

#[spacetimedb::reducer]
pub fn delete_unique_0_u32_u64_str_by_id(id: u32) {
    unique_0_u32_u64_str::delete_by_id(&id);
}

#[spacetimedb::reducer]
pub fn delete_unique_0_u32_u64_u64_by_id(id: u32) {
    unique_0_u32_u64_u64::delete_by_id(&id);
}

// ---------- clear table ----------
#[spacetimedb::reducer]
pub fn clear_table_unique_0_u32_u64_str() {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_no_index_u32_u64_str() {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_btree_each_column_u32_u64_str() {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_unique_0_u32_u64_u64() {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_no_index_u32_u64_u64() {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_btree_each_column_u32_u64_u64() {
    unimplemented!("Modules currently have no interface to clear a table");
}
// ---------- count ----------

// You need to inspect the module outputs to actually read the result from these.

#[spacetimedb::reducer]
pub fn count_unique_0_u32_u64_str() {
    println!("COUNT: {}", unique_0_u32_u64_str::iter().count());
}

#[spacetimedb::reducer]
pub fn count_no_index_u32_u64_str() {
    println!("COUNT: {}", no_index_u32_u64_str::iter().count());
}

#[spacetimedb::reducer]
pub fn count_btree_each_column_u32_u64_str() {
    println!("COUNT: {}", btree_each_column_u32_u64_str::iter().count());
}

#[spacetimedb::reducer]
pub fn count_unique_0_u32_u64_u64() {
    println!("COUNT: {}", unique_0_u32_u64_u64::iter().count());
}

#[spacetimedb::reducer]
pub fn count_no_index_u32_u64_u64() {
    println!("COUNT: {}", no_index_u32_u64_u64::iter().count());
}

#[spacetimedb::reducer]
pub fn count_btree_each_column_u32_u64_u64() {
    println!("COUNT: {}", btree_each_column_u32_u64_u64::iter().count());
}
// ---------- module-specific stuff ----------

#[spacetimedb::reducer]
pub fn fn_with_1_args(_arg: String) {}

#[spacetimedb::reducer]
pub fn fn_with_32_args(
    _arg1: String,
    _arg2: String,
    _arg3: String,
    _arg4: String,
    _arg5: String,
    _arg6: String,
    _arg7: String,
    _arg8: String,
    _arg9: String,
    _arg10: String,
    _arg11: String,
    _arg12: String,
    _arg13: String,
    _arg14: String,
    _arg15: String,
    _arg16: String,
    _arg17: String,
    _arg18: String,
    _arg19: String,
    _arg20: String,
    _arg21: String,
    _arg22: String,
    _arg23: String,
    _arg24: String,
    _arg25: String,
    _arg26: String,
    _arg27: String,
    _arg28: String,
    _arg29: String,
    _arg30: String,
    _arg31: String,
    _arg32: String,
) {
}

#[spacetimedb::reducer]
pub fn print_many_things(n: u32) {
    for _ in 0..n {
        println!("hello again!");
    }
}
