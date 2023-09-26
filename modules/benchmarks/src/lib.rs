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
//! - `Unique` / `unique`: a single unique key, declared first in the struct.
//! - `NonUnique` / `non_unique`: no indexes.
//! - `MultiIndex` / `multi_index`: one index for each row.
//!
//! Obviously more could be added...

#![allow(clippy::too_many_arguments, unused_variables)]

use spacetimedb::{println, spacetimedb};
use std::hint::black_box;

// The following piece of code must remain synced

// ---------- SYNCED CODE ----------

#[spacetimedb(table)]
pub struct UniquePerson {
    #[unique]
    id: u32,
    name: String,
    age: u64,
}

#[spacetimedb(table)]
pub struct NonUniquePerson {
    id: u32,
    name: String,
    age: u64,
}

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "id", id))]
#[spacetimedb(index(btree, name = "name", name))]
#[spacetimedb(index(btree, name = "age", age))]
pub struct MultiIndexPerson {
    id: u32,
    name: String,
    age: u64,
}

#[spacetimedb(table)]
pub struct UniqueLocation {
    #[unique]
    id: u32,
    x: u64,
    y: u64,
}

#[spacetimedb(table)]
pub struct NonUniqueLocation {
    id: u32,
    x: u64,
    y: u64,
}

#[spacetimedb(table)]
#[spacetimedb(index(btree, name = "id", id))]
#[spacetimedb(index(btree, name = "x", x))]
#[spacetimedb(index(btree, name = "y", y))]
pub struct MultiIndexLocation {
    id: u32,
    x: u64,
    y: u64,
}
// ---------- / SYNCED CODE ----------

#[spacetimedb(reducer)]
pub fn empty() {}

// ---------- insert ----------
#[spacetimedb(reducer)]
pub fn insert_unique_person(id: u32, name: String, age: u64) {
    UniquePerson::insert(UniquePerson { id, name, age }).unwrap();
}

#[spacetimedb(reducer)]
pub fn insert_non_unique_person(id: u32, name: String, age: u64) {
    NonUniquePerson::insert(NonUniquePerson { id, name, age });
}

#[spacetimedb(reducer)]
pub fn insert_multi_index_person(id: u32, name: String, age: u64) {
    MultiIndexPerson::insert(MultiIndexPerson { id, name, age });
}

#[spacetimedb(reducer)]
pub fn insert_unique_location(id: u32, x: u64, y: u64) {
    UniqueLocation::insert(UniqueLocation { id, x, y }).unwrap();
}

#[spacetimedb(reducer)]
pub fn insert_non_unique_location(id: u32, x: u64, y: u64) {
    NonUniqueLocation::insert(NonUniqueLocation { id, x, y });
}

#[spacetimedb(reducer)]
pub fn insert_multi_index_location(id: u32, x: u64, y: u64) {
    MultiIndexLocation::insert(MultiIndexLocation { id, x, y });
}

// ---------- insert bulk ----------

#[spacetimedb(reducer)]
pub fn insert_bulk_unique_location(locs: Vec<UniqueLocation>) {
    for loc in locs {
        UniqueLocation::insert(loc).unwrap();
    }
}

#[spacetimedb(reducer)]
pub fn insert_bulk_non_unique_location(locs: Vec<NonUniqueLocation>) {
    for loc in locs {
        NonUniqueLocation::insert(loc);
    }
}

#[spacetimedb(reducer)]
pub fn insert_bulk_multi_index_location(locs: Vec<MultiIndexLocation>) {
    for loc in locs {
        MultiIndexLocation::insert(loc);
    }
}

#[spacetimedb(reducer)]
pub fn insert_bulk_unique_person(people: Vec<UniquePerson>) {
    for person in people {
        UniquePerson::insert(person).unwrap();
    }
}

#[spacetimedb(reducer)]
pub fn insert_bulk_non_unique_person(people: Vec<NonUniquePerson>) {
    for person in people {
        NonUniquePerson::insert(person);
    }
}

#[spacetimedb(reducer)]
pub fn insert_bulk_multi_index_person(people: Vec<MultiIndexPerson>) {
    for person in people {
        MultiIndexPerson::insert(person);
    }
}

// ---------- iterate ----------

#[spacetimedb(reducer)]
pub fn iterate_unique_person() {
    for person in UniquePerson::iter() {
        black_box(person);
    }
}
#[spacetimedb(reducer)]
pub fn iterate_unique_location() {
    for location in UniqueLocation::iter() {
        black_box(location);
    }
}

// ---------- filtering ----------

#[spacetimedb(reducer)]
pub fn filter_unique_person_by_id(id: u32) {
    if let Some(p) = UniquePerson::filter_by_id(&id) {
        black_box(p);
    }
}

#[spacetimedb(reducer)]
pub fn filter_non_unique_person_by_id(id: u32) {
    for p in NonUniquePerson::filter_by_id(&id) {
        black_box(p);
    }
}

#[spacetimedb(reducer)]
pub fn filter_multi_index_person_by_id(id: u32) {
    for p in MultiIndexPerson::filter_by_id(&id) {
        black_box(p);
    }
}

#[spacetimedb(reducer)]
pub fn filter_unique_person_by_name(name: String) {
    for p in UniquePerson::filter_by_name(&name) {
        black_box(p);
    }
}

#[spacetimedb(reducer)]
pub fn filter_non_unique_person_by_name(name: String) {
    for p in NonUniquePerson::filter_by_name(&name) {
        black_box(p);
    }
}

#[spacetimedb(reducer)]
pub fn filter_multi_index_person_by_name(name: String) {
    for p in MultiIndexPerson::filter_by_name(&name) {
        black_box(p);
    }
}

#[spacetimedb(reducer)]
pub fn filter_unique_location_by_id(id: u32) {
    if let Some(loc) = UniqueLocation::filter_by_id(&id) {
        black_box(loc);
    }
}

#[spacetimedb(reducer)]
pub fn filter_non_unique_location_by_id(id: u32) {
    for loc in NonUniqueLocation::filter_by_id(&id) {
        black_box(loc);
    }
}

#[spacetimedb(reducer)]
pub fn filter_multi_index_location_by_id(id: u32) {
    for loc in MultiIndexLocation::filter_by_id(&id) {
        black_box(loc);
    }
}

#[spacetimedb(reducer)]
pub fn filter_unique_location_by_x(x: u64) {
    for loc in UniqueLocation::filter_by_x(&x) {
        black_box(loc);
    }
}

#[spacetimedb(reducer)]
pub fn filter_non_unique_location_by_x(x: u64) {
    for loc in NonUniqueLocation::filter_by_x(&x) {
        black_box(loc);
    }
}

#[spacetimedb(reducer)]
pub fn filter_multi_index_location_by_x(x: u64) {
    for loc in MultiIndexLocation::filter_by_x(&x) {
        black_box(loc);
    }
}

// ---------- delete ----------

// FIXME: current nonunique delete interface is UNUSABLE!!!!

#[spacetimedb(reducer)]
pub fn delete_unique_person_by_id(id: u32) {
    UniquePerson::delete_by_id(&id);
}

#[spacetimedb(reducer)]
pub fn delete_unique_location_by_id(id: u32) {
    UniqueLocation::delete_by_id(&id);
}

// ---------- clear table ----------
#[spacetimedb(reducer)]
pub fn clear_table_unique_person() {
    UniquePerson::delete(|_| true);
}

#[spacetimedb(reducer)]
pub fn clear_table_non_unique_person() {
    NonUniquePerson::delete(|_| true);
}

#[spacetimedb(reducer)]
pub fn clear_table_multi_index_person() {
    MultiIndexPerson::delete(|_| true);
}

#[spacetimedb(reducer)]
pub fn clear_table_unique_location() {
    UniqueLocation::delete(|_| true);
}

#[spacetimedb(reducer)]
pub fn clear_table_non_unique_location() {
    NonUniqueLocation::delete(|_| true);
}

#[spacetimedb(reducer)]
pub fn clear_table_multi_index_location() {
    MultiIndexLocation::delete(|_| true);
}
// ---------- count ----------

// You need to inspect the module outputs to actually read the result from these.

#[spacetimedb(reducer)]
pub fn count_unique_person() {
    println!("COUNT: {}", UniquePerson::iter().count());
}

#[spacetimedb(reducer)]
pub fn count_non_unique_person() {
    println!("COUNT: {}", NonUniquePerson::iter().count());
}

#[spacetimedb(reducer)]
pub fn count_multi_index_person() {
    println!("COUNT: {}", MultiIndexPerson::iter().count());
}

#[spacetimedb(reducer)]
pub fn count_unique_location() {
    println!("COUNT: {}", UniqueLocation::iter().count());
}

#[spacetimedb(reducer)]
pub fn count_non_unique_location() {
    println!("COUNT: {}", NonUniqueLocation::iter().count());
}

#[spacetimedb(reducer)]
pub fn count_multi_index_location() {
    println!("COUNT: {}", MultiIndexLocation::iter().count());
}
// ---------- module-specific stuff ----------

#[spacetimedb(reducer)]
pub fn fn_with_1_args(_arg: String) {}

#[spacetimedb(reducer)]
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

#[spacetimedb(reducer)]
pub fn print_many_things(n: u32) {
    for _ in 0..n {
        println!("hello again!");
    }
}
