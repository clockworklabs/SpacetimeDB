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
use fake::faker::address::raw::{CityName, CountryName, StateName, ZipCode};
use fake::faker::internet::raw::{Password, SafeEmail};
use fake::faker::lorem::raw::{Paragraph, Words};
use fake::faker::name::raw::*;
use fake::faker::phone_number::raw::CellNumber;
use fake::locales::EN;
use fake::{Fake, Faker};
use spacetimedb::rand::Rng;
use spacetimedb::{log, ConnectionId, Identity, ReducerContext, SpacetimeType, StdbRng, Table};
use std::hint::black_box;
// ---------- schemas ----------

#[spacetimedb::table(name = unique_0_u32_u64_str)]
pub struct unique_0_u32_u64_str_t {
    #[unique]
    id: u32,
    age: u64,
    name: String,
}

#[spacetimedb::table(name = no_index_u32_u64_str)]
pub struct no_index_u32_u64_str_t {
    id: u32,
    age: u64,
    name: String,
}

#[spacetimedb::table(name = btree_each_column_u32_u64_str)]
pub struct btree_each_column_u32_u64_str_t {
    #[index(btree)]
    id: u32,
    #[index(btree)]
    age: u64,
    #[index(btree)]
    name: String,
}

#[spacetimedb::table(name = unique_0_u32_u64_u64)]
pub struct unique_0_u32_u64_u64_t {
    #[unique]
    id: u32,
    x: u64,
    y: u64,
}

#[spacetimedb::table(name = no_index_u32_u64_u64)]
pub struct no_index_u32_u64_u64_t {
    id: u32,
    x: u64,
    y: u64,
}

#[spacetimedb::table(name = btree_each_column_u32_u64_u64)]
pub struct btree_each_column_u32_u64_u64_t {
    #[index(btree)]
    id: u32,
    #[index(btree)]
    x: u64,
    #[index(btree)]
    y: u64,
}

// Tables for data generation loading tests

#[derive(SpacetimeType)]
pub enum Load {
    Tiny,
    Small,
    Medium,
    Large,
}

#[derive(SpacetimeType)]
pub enum Index {
    One,
    Many,
}

#[spacetimedb::table(name = tiny_rows)]
pub struct tiny_rows_t {
    #[index(btree)]
    id: u8,
}

#[spacetimedb::table(name = small_rows)]
pub struct small_rows_t {
    #[index(btree)]
    id: u64,
    x: u64,
    y: u64,
}

#[spacetimedb::table(name = small_btree_each_column_rows)]
pub struct small_rows_btree_each_column_t {
    #[index(btree)]
    id: u64,
    #[index(btree)]
    x: u64,
    #[index(btree)]
    y: u64,
}

#[spacetimedb::table(name = medium_var_rows)]
pub struct medium_var_rows_t {
    #[index(btree)]
    id: u64,
    name: String,
    email: String,
    password: String,
    identity: Identity,
    connection: ConnectionId,
    pos: Vec<u64>,
}

#[spacetimedb::table(name = medium_var_rows_btree_each_column)]
pub struct medium_var_rows_btree_each_column_t {
    #[index(btree)]
    id: u64,
    #[index(btree)]
    name: String,
    #[index(btree)]
    email: String,
    #[index(btree)]
    password: String,
    #[index(btree)]
    identity: Identity,
    #[index(btree)]
    connection: ConnectionId,
    #[index(btree)]
    pos: Vec<u64>,
}

#[spacetimedb::table(name = large_var_rows)]
pub struct large_var_rows_t {
    #[index(btree)]
    id: u128,
    invoice_code: String,
    status: String,
    customer: Identity,
    company: ConnectionId,
    user_name: String,

    price: f64,
    cost: f64,
    discount: f64,
    taxes: Vec<f64>,
    tax_total: f64,
    sub_total: f64,
    total: f64,

    country: String,
    state: String,
    city: String,
    zip_code: Option<String>,
    phone: String,

    notes: String,
    tags: Option<Vec<String>>,
}

#[spacetimedb::table(name = large_var_rows_btree_each_column)]
pub struct large_var_rows_btree_each_column_t {
    #[index(btree)]
    id: u128,
    #[index(btree)]
    invoice_code: String,
    #[index(btree)]
    status: String,
    #[index(btree)]
    customer: Identity,
    #[index(btree)]
    company: ConnectionId,
    #[index(btree)]
    user_name: String,

    #[index(btree)]
    price: f64,
    #[index(btree)]
    cost: f64,
    #[index(btree)]
    discount: f64,
    #[index(btree)]
    taxes: Vec<f64>,
    #[index(btree)]
    tax_total: f64,
    #[index(btree)]
    sub_total: f64,
    #[index(btree)]
    total: f64,

    #[index(btree)]
    country: String,
    #[index(btree)]
    state: String,
    #[index(btree)]
    city: String,
    #[index(btree)]
    zip_code: Option<String>,
    #[index(btree)]
    phone: String,

    #[index(btree)]
    notes: String,
    #[index(btree)]
    tags: Option<Vec<String>>,
}

// ---------- empty ----------

#[spacetimedb::reducer]
pub fn empty(_ctx: &ReducerContext) {}

// ---------- insert ----------
#[spacetimedb::reducer]
pub fn insert_unique_0_u32_u64_str(ctx: &ReducerContext, id: u32, age: u64, name: String) {
    ctx.db
        .unique_0_u32_u64_str()
        .insert(unique_0_u32_u64_str_t { id, name, age });
}

#[spacetimedb::reducer]
pub fn insert_no_index_u32_u64_str(ctx: &ReducerContext, id: u32, age: u64, name: String) {
    ctx.db
        .no_index_u32_u64_str()
        .insert(no_index_u32_u64_str_t { id, name, age });
}

#[spacetimedb::reducer]
pub fn insert_btree_each_column_u32_u64_str(ctx: &ReducerContext, id: u32, age: u64, name: String) {
    ctx.db
        .btree_each_column_u32_u64_str()
        .insert(btree_each_column_u32_u64_str_t { id, name, age });
}

#[spacetimedb::reducer]
pub fn insert_unique_0_u32_u64_u64(ctx: &ReducerContext, id: u32, x: u64, y: u64) {
    ctx.db
        .unique_0_u32_u64_u64()
        .insert(unique_0_u32_u64_u64_t { id, x, y });
}

#[spacetimedb::reducer]
pub fn insert_no_index_u32_u64_u64(ctx: &ReducerContext, id: u32, x: u64, y: u64) {
    ctx.db
        .no_index_u32_u64_u64()
        .insert(no_index_u32_u64_u64_t { id, x, y });
}

#[spacetimedb::reducer]
pub fn insert_btree_each_column_u32_u64_u64(ctx: &ReducerContext, id: u32, x: u64, y: u64) {
    ctx.db
        .btree_each_column_u32_u64_u64()
        .insert(btree_each_column_u32_u64_u64_t { id, x, y });
}

// ---------- insert bulk ----------

#[spacetimedb::reducer]
pub fn insert_bulk_unique_0_u32_u64_u64(ctx: &ReducerContext, locs: Vec<unique_0_u32_u64_u64_t>) {
    for loc in locs {
        ctx.db.unique_0_u32_u64_u64().insert(loc);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_no_index_u32_u64_u64(ctx: &ReducerContext, locs: Vec<no_index_u32_u64_u64_t>) {
    for loc in locs {
        ctx.db.no_index_u32_u64_u64().insert(loc);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_btree_each_column_u32_u64_u64(ctx: &ReducerContext, locs: Vec<btree_each_column_u32_u64_u64_t>) {
    for loc in locs {
        ctx.db.btree_each_column_u32_u64_u64().insert(loc);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_unique_0_u32_u64_str(ctx: &ReducerContext, people: Vec<unique_0_u32_u64_str_t>) {
    for u32_u64_str in people {
        ctx.db.unique_0_u32_u64_str().insert(u32_u64_str);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_no_index_u32_u64_str(ctx: &ReducerContext, people: Vec<no_index_u32_u64_str_t>) {
    for u32_u64_str in people {
        ctx.db.no_index_u32_u64_str().insert(u32_u64_str);
    }
}

#[spacetimedb::reducer]
pub fn insert_bulk_btree_each_column_u32_u64_str(ctx: &ReducerContext, people: Vec<btree_each_column_u32_u64_str_t>) {
    for u32_u64_str in people {
        ctx.db.btree_each_column_u32_u64_str().insert(u32_u64_str);
    }
}

fn rand_connection_id(rng: &mut &StdbRng) -> ConnectionId {
    ConnectionId::from(Faker.fake_with_rng::<u128, _>(rng))
}

fn rand_identity(rng: &mut &StdbRng) -> Identity {
    Identity::from_u256(Faker.fake_with_rng::<u128, _>(rng).into())
}

#[spacetimedb::reducer]
pub fn insert_bulk_tiny_rows(ctx: &ReducerContext, rows: u8) {
    for id in 0..rows {
        ctx.db.tiny_rows().insert(tiny_rows_t { id });
    }
    log::info!("Inserted on tiny_rows: {} rows", rows);
}

#[spacetimedb::reducer]
pub fn insert_bulk_small_rows(ctx: &ReducerContext, rows: u64) {
    let mut rng = ctx.rng();
    for id in 0..rows {
        ctx.db.small_rows().insert(small_rows_t {
            id,
            x: rng.gen(),
            y: rng.gen(),
        });
    }
    log::info!("Inserted on small_rows: {} rows", rows);
}

#[spacetimedb::reducer]
pub fn insert_bulk_small_btree_each_column_rows(ctx: &ReducerContext, rows: u64) {
    let mut rng = ctx.rng();
    for id in 0..rows {
        ctx.db
            .small_btree_each_column_rows()
            .insert(small_rows_btree_each_column_t {
                id,
                x: rng.gen(),
                y: rng.gen(),
            });
    }
    log::info!("Inserted on small_btree_each_column_rows: {} rows", rows);
}

#[spacetimedb::reducer]
pub fn insert_bulk_medium_var_rows(ctx: &ReducerContext, rows: u64) {
    let mut rng = ctx.rng();
    for id in 0..rows {
        ctx.db.medium_var_rows().insert(medium_var_rows_t {
            id,
            name: Name(EN).fake_with_rng(&mut rng),
            email: SafeEmail(EN).fake_with_rng(&mut rng),
            password: Password(EN, 6..10).fake_with_rng(&mut rng),
            identity: rand_identity(&mut rng),
            connection: rand_connection_id(&mut rng),
            pos: Faker.fake_with_rng(&mut rng),
        });
    }
    log::info!("Inserted on medium_var_rows: {} rows", rows);
}

#[spacetimedb::reducer]
pub fn insert_bulk_medium_var_rows_btree_each_column(ctx: &ReducerContext, rows: u64) {
    let mut rng = ctx.rng();
    for id in 0..rows {
        ctx.db
            .medium_var_rows_btree_each_column()
            .insert(medium_var_rows_btree_each_column_t {
                id,
                name: Name(EN).fake_with_rng(&mut rng),
                email: SafeEmail(EN).fake_with_rng(&mut rng),
                password: Password(EN, 6..10).fake_with_rng(&mut rng),
                identity: rand_identity(&mut rng),
                connection: rand_connection_id(&mut rng),
                pos: Faker.fake_with_rng(&mut rng),
            });
    }
    log::info!("Inserted on medium_var_rows_btree_each_column: {} rows", rows);
}

#[spacetimedb::reducer]
pub fn insert_bulk_large_var_rows(ctx: &ReducerContext, rows: u64) {
    let mut rng = ctx.rng();
    for id in 0..(rows as u128) {
        ctx.db.large_var_rows().insert(large_var_rows_t {
            id,
            invoice_code: Faker.fake_with_rng(&mut rng),
            status: Faker.fake_with_rng(&mut rng),
            customer: rand_identity(&mut rng),
            company: rand_connection_id(&mut rng),
            user_name: Faker.fake_with_rng(&mut rng),

            price: Faker.fake_with_rng(&mut rng),
            cost: Faker.fake_with_rng(&mut rng),
            discount: Faker.fake_with_rng(&mut rng),
            taxes: Faker.fake_with_rng(&mut rng),
            tax_total: Faker.fake_with_rng(&mut rng),
            sub_total: Faker.fake_with_rng(&mut rng),
            total: Faker.fake_with_rng(&mut rng),

            country: CountryName(EN).fake_with_rng(&mut rng),
            state: StateName(EN).fake_with_rng(&mut rng),
            city: CityName(EN).fake_with_rng(&mut rng),
            zip_code: ZipCode(EN).fake_with_rng(&mut rng),
            phone: CellNumber(EN).fake_with_rng(&mut rng),

            notes: Paragraph(EN, 0..3).fake_with_rng(&mut rng),
            tags: Words(EN, 0..3).fake_with_rng(&mut rng),
        });
    }
    log::info!("Inserted on large_var_rows: {} rows", rows);
}

#[spacetimedb::reducer]
pub fn insert_bulk_large_var_rows_btree_each_column(ctx: &ReducerContext, rows: u64) {
    let mut rng = ctx.rng();
    for id in 0..(rows as u128) {
        ctx.db
            .large_var_rows_btree_each_column()
            .insert(large_var_rows_btree_each_column_t {
                id,
                invoice_code: Faker.fake_with_rng(&mut rng),
                status: Faker.fake_with_rng(&mut rng),
                customer: rand_identity(&mut rng),
                company: rand_connection_id(&mut rng),
                user_name: Faker.fake_with_rng(&mut rng),

                price: Faker.fake_with_rng(&mut rng),
                cost: Faker.fake_with_rng(&mut rng),
                discount: Faker.fake_with_rng(&mut rng),
                taxes: Faker.fake_with_rng(&mut rng),
                tax_total: Faker.fake_with_rng(&mut rng),
                sub_total: Faker.fake_with_rng(&mut rng),
                total: Faker.fake_with_rng(&mut rng),

                country: CountryName(EN).fake_with_rng(&mut rng),
                state: StateName(EN).fake_with_rng(&mut rng),
                city: CityName(EN).fake_with_rng(&mut rng),
                zip_code: ZipCode(EN).fake_with_rng(&mut rng),
                phone: CellNumber(EN).fake_with_rng(&mut rng),

                notes: Paragraph(EN, 0..3).fake_with_rng(&mut rng),
                tags: Words(EN, 0..3).fake_with_rng(&mut rng),
            });
    }
    log::info!("Inserted on large_var_rows_btree_each_column: {} rows", rows);
}

/// This reducer is used to load synthetic data into the database for benchmarking purposes.
///
/// The input is a string with the following format:
///
/// `load_type`: [`Load`], `index_type`: [`Index`], `row_count`: `u32`
#[spacetimedb::reducer]
pub fn load(ctx: &ReducerContext, input: String) -> Result<(), String> {
    let args = input.split(',').map(|x| x.trim().to_lowercase()).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(format!("Expected 3 arguments, got {}", args.len()));
    }
    let load = match args[0].as_str() {
        "tiny" => Load::Tiny,
        "small" => Load::Small,
        "medium" => Load::Medium,
        "large" => Load::Large,
        x => {
            return Err(format!(
                "Invalid load type: '{x}', expected: tiny, small, medium, or large"
            ))
        }
    };
    let index = match args[1].as_str() {
        "one" => Index::One,
        "many" => Index::Many,
        x => return Err(format!("Invalid index type: '{x}', expected: one, or many")),
    };
    let rows = args[2]
        .parse::<u64>()
        .map_err(|e| format!("Invalid row count: {}", e))?;

    match (load, index) {
        (Load::Tiny, Index::One | Index::Many) => insert_bulk_tiny_rows(ctx, rows as u8),
        (Load::Small, Index::One) => insert_bulk_small_rows(ctx, rows),
        (Load::Small, Index::Many) => insert_bulk_small_btree_each_column_rows(ctx, rows),
        (Load::Medium, Index::One) => insert_bulk_medium_var_rows(ctx, rows),
        (Load::Medium, Index::Many) => insert_bulk_medium_var_rows_btree_each_column(ctx, rows),
        (Load::Large, Index::One) => insert_bulk_large_var_rows(ctx, rows),
        (Load::Large, Index::Many) => insert_bulk_large_var_rows_btree_each_column(ctx, rows),
    }

    Ok(())
}

/// Used to execute a series of reducers in sequence for benchmarking purposes.
///
/// The input is a string with the following format:
///
/// `load_type`: [`Load`], `inserts`: `u32`, `query`: `u32`, `deletes`: `u32`
///
/// The order of the `inserts`, `query`, and `deletes` can be changed and will be executed in that order.
#[spacetimedb::reducer]
pub fn queries(ctx: &ReducerContext, input: String) -> Result<(), String> {
    let args = input.split(',').map(|x| x.trim().to_lowercase()).collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(format!("Expected at least 2 arguments, got {}", args.len()));
    }
    let load = match args[0].as_str() {
        "tiny" => Load::Tiny,
        "small" => Load::Small,
        "medium" => Load::Medium,
        "large" => Load::Large,
        x => {
            return Err(format!(
                "Invalid load type: '{x}', expected: tiny, small, medium, or large"
            ))
        }
    };

    let mut inserts = 0u64;
    let mut queries = 0u64;
    let mut deletes = 0u64;

    for arg in args.iter().skip(1) {
        let parts = arg.split(':').map(|x| x.trim()).collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(format!("Invalid argument: '{arg}', expected: 'operation:count'"));
        }
        let count = parts[1].parse::<u64>().map_err(|e| format!("Invalid count: {}", e))?;
        match parts[0] {
            "inserts" => inserts = count,
            "query" => queries = count,
            "deletes" => deletes = count,
            x => {
                return Err(format!(
                    "Invalid operation: '{x}', expected: inserts, query, or deletes"
                ))
            }
        }
    }

    log::info!("Executing queries: inserts: {inserts}, query: {queries}, deletes: {deletes}");
    // To allow to insert duplicate rows, the `ids` not use `[unique]` attribute, causing to not be able to use `update` method
    match load {
        Load::Tiny => {
            if inserts > 0 {
                insert_bulk_tiny_rows(ctx, inserts as u8);
            }
            for id in 0..queries {
                filter_tiny_rows_by_id(ctx, id as u8);
            }
            for id in 0..deletes {
                delete_tiny_rows_by_id(ctx, id as u8);
            }
        }
        Load::Small => {
            if inserts > 0 {
                insert_bulk_small_rows(ctx, inserts);
            }
            for id in 0..queries {
                filter_small_rows_by_id(ctx, id);
            }
            for id in 0..deletes {
                delete_small_rows_by_id(ctx, id);
            }
        }
        Load::Medium => {
            if inserts > 0 {
                insert_bulk_medium_var_rows(ctx, inserts);
            }
            for id in 0..queries {
                filter_medium_var_rows_by_id(ctx, id);
            }
            for id in 0..deletes {
                delete_medium_var_rows_by_id(ctx, id);
            }
        }
        Load::Large => {
            if inserts > 0 {
                insert_bulk_large_var_rows(ctx, inserts);
            }
            for id in 0..queries {
                filter_large_var_rows_by_id(ctx, id as u128);
            }
            for id in 0..deletes {
                delete_large_var_rows_by_id(ctx, id as u128);
            }
        }
    }

    Ok(())
}
// ---------- update ----------

#[spacetimedb::reducer]
pub fn update_bulk_unique_0_u32_u64_u64(ctx: &ReducerContext, row_count: u32) {
    let mut hit: u32 = 0;
    for loc in ctx.db.unique_0_u32_u64_u64().iter().take(row_count as usize) {
        hit += 1;
        ctx.db.unique_0_u32_u64_u64().id().update(unique_0_u32_u64_u64_t {
            id: loc.id,
            x: loc.x.wrapping_add(1),
            y: loc.y,
        });
    }
    assert_eq!(hit, row_count, "not enough rows to perform requested amount of updates");
}

#[spacetimedb::reducer]
pub fn update_bulk_unique_0_u32_u64_str(ctx: &ReducerContext, row_count: u32) {
    let mut hit: u32 = 0;
    for u32_u64_str in ctx.db.unique_0_u32_u64_str().iter().take(row_count as usize) {
        hit += 1;
        ctx.db.unique_0_u32_u64_str().id().update(unique_0_u32_u64_str_t {
            id: u32_u64_str.id,
            name: u32_u64_str.name,
            age: u32_u64_str.age.wrapping_add(1),
        });
    }
    assert_eq!(hit, row_count, "not enough rows to perform requested amount of updates");
}

// ---------- iterate ----------

#[spacetimedb::reducer]
pub fn iterate_unique_0_u32_u64_str(ctx: &ReducerContext) {
    for u32_u64_str in ctx.db.unique_0_u32_u64_str().iter() {
        black_box(u32_u64_str);
    }
}
#[spacetimedb::reducer]
pub fn iterate_unique_0_u32_u64_u64(ctx: &ReducerContext) {
    for u32_u64_u64 in ctx.db.unique_0_u32_u64_u64().iter() {
        black_box(u32_u64_u64);
    }
}

// ---------- filtering ----------

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_str_by_id(ctx: &ReducerContext, id: u32) {
    if let Some(p) = ctx.db.unique_0_u32_u64_str().id().find(id) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_str_by_id(ctx: &ReducerContext, id: u32) {
    for p in ctx.db.no_index_u32_u64_str().iter().filter(|p| p.id == id) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_str_by_id(ctx: &ReducerContext, id: u32) {
    for p in ctx.db.btree_each_column_u32_u64_str().id().filter(&id) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_str_by_name(ctx: &ReducerContext, name: String) {
    for p in ctx.db.unique_0_u32_u64_str().iter().filter(|p| p.name == name) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_str_by_name(ctx: &ReducerContext, name: String) {
    for p in ctx.db.no_index_u32_u64_str().iter().filter(|p| p.name == name) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_str_by_name(ctx: &ReducerContext, name: String) {
    for p in ctx.db.btree_each_column_u32_u64_str().name().filter(&name) {
        black_box(p);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_u64_by_id(ctx: &ReducerContext, id: u32) {
    if let Some(loc) = ctx.db.unique_0_u32_u64_u64().id().find(id) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_u64_by_id(ctx: &ReducerContext, id: u32) {
    for loc in ctx.db.no_index_u32_u64_u64().iter().filter(|p| p.id == id) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_u64_by_id(ctx: &ReducerContext, id: u32) {
    for loc in ctx.db.btree_each_column_u32_u64_u64().id().filter(&id) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_u64_by_x(ctx: &ReducerContext, x: u64) {
    for loc in ctx.db.unique_0_u32_u64_u64().iter().filter(|p| p.x == x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_u64_by_x(ctx: &ReducerContext, x: u64) {
    for loc in ctx.db.no_index_u32_u64_u64().iter().filter(|p| p.x == x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_u64_by_x(ctx: &ReducerContext, x: u64) {
    for loc in ctx.db.btree_each_column_u32_u64_u64().x().filter(&x) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_unique_0_u32_u64_u64_by_y(ctx: &ReducerContext, y: u64) {
    for loc in ctx.db.unique_0_u32_u64_u64().iter().filter(|p| p.y == y) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_no_index_u32_u64_u64_by_y(ctx: &ReducerContext, y: u64) {
    for loc in ctx.db.no_index_u32_u64_u64().iter().filter(|p| p.y == y) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_btree_each_column_u32_u64_u64_by_y(ctx: &ReducerContext, y: u64) {
    for loc in ctx.db.btree_each_column_u32_u64_u64().y().filter(&y) {
        black_box(loc);
    }
}

#[spacetimedb::reducer]
pub fn filter_tiny_rows_by_id(ctx: &ReducerContext, id: u8) {
    for row in ctx.db.tiny_rows().iter().filter(|r| r.id == id) {
        black_box(row);
    }
}

#[spacetimedb::reducer]
pub fn filter_small_rows_by_id(ctx: &ReducerContext, id: u64) {
    for row in ctx.db.small_rows().iter().filter(|r| r.id == id) {
        black_box(row);
    }
}

#[spacetimedb::reducer]
pub fn filter_small_btree_each_column_rows_by_id(ctx: &ReducerContext, id: u64) {
    for row in ctx.db.small_btree_each_column_rows().iter().filter(|r| r.id == id) {
        black_box(row);
    }
}

#[spacetimedb::reducer]
pub fn filter_medium_var_rows_by_id(ctx: &ReducerContext, id: u64) {
    for row in ctx.db.medium_var_rows().iter().filter(|r| r.id == id) {
        black_box(row);
    }
}

#[spacetimedb::reducer]
pub fn filter_medium_var_rows_btree_each_column_by_id(ctx: &ReducerContext, id: u64) {
    for row in ctx.db.medium_var_rows_btree_each_column().iter().filter(|r| r.id == id) {
        black_box(row);
    }
}

#[spacetimedb::reducer]
pub fn filter_large_var_rows_by_id(ctx: &ReducerContext, id: u128) {
    for row in ctx.db.large_var_rows().iter().filter(|r| r.id == id) {
        black_box(row);
    }
}

#[spacetimedb::reducer]
pub fn filter_large_var_rows_btree_each_column_by_id(ctx: &ReducerContext, id: u128) {
    for row in ctx.db.large_var_rows_btree_each_column().iter().filter(|r| r.id == id) {
        black_box(row);
    }
}

// ---------- delete ----------

// FIXME: current nonunique delete interface is UNUSABLE!!!!

#[spacetimedb::reducer]
pub fn delete_unique_0_u32_u64_str_by_id(ctx: &ReducerContext, id: u32) {
    ctx.db.unique_0_u32_u64_str().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_unique_0_u32_u64_u64_by_id(ctx: &ReducerContext, id: u32) {
    ctx.db.unique_0_u32_u64_u64().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_tiny_rows_by_id(ctx: &ReducerContext, id: u8) {
    ctx.db.tiny_rows().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_small_rows_by_id(ctx: &ReducerContext, id: u64) {
    ctx.db.small_rows().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_small_btree_each_column_rows_by_id(ctx: &ReducerContext, id: u64) {
    ctx.db.small_btree_each_column_rows().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_medium_var_rows_by_id(ctx: &ReducerContext, id: u64) {
    ctx.db.medium_var_rows().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_medium_var_rows_btree_each_column_by_id(ctx: &ReducerContext, id: u64) {
    ctx.db.medium_var_rows_btree_each_column().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_large_var_rows_by_id(ctx: &ReducerContext, id: u128) {
    ctx.db.large_var_rows().id().delete(id);
}

#[spacetimedb::reducer]
pub fn delete_large_var_rows_btree_each_column_by_id(ctx: &ReducerContext, id: u128) {
    ctx.db.large_var_rows_btree_each_column().id().delete(id);
}

// ---------- clear table ----------
#[spacetimedb::reducer]
pub fn clear_table_unique_0_u32_u64_str(_ctx: &ReducerContext) {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_no_index_u32_u64_str(_ctx: &ReducerContext) {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_btree_each_column_u32_u64_str(_ctx: &ReducerContext) {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_unique_0_u32_u64_u64(_ctx: &ReducerContext) {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_no_index_u32_u64_u64(_ctx: &ReducerContext) {
    unimplemented!("Modules currently have no interface to clear a table");
}

#[spacetimedb::reducer]
pub fn clear_table_btree_each_column_u32_u64_u64(_ctx: &ReducerContext) {
    unimplemented!("Modules currently have no interface to clear a table");
}
// ---------- count ----------

// You need to inspect the module outputs to actually read the result from these.

#[spacetimedb::reducer]
pub fn count_unique_0_u32_u64_str(ctx: &ReducerContext) {
    log::info!("COUNT: {}", ctx.db.unique_0_u32_u64_str().count());
}

#[spacetimedb::reducer]
pub fn count_no_index_u32_u64_str(ctx: &ReducerContext) {
    log::info!("COUNT: {}", ctx.db.no_index_u32_u64_str().count());
}

#[spacetimedb::reducer]
pub fn count_btree_each_column_u32_u64_str(ctx: &ReducerContext) {
    log::info!("COUNT: {}", ctx.db.btree_each_column_u32_u64_str().count());
}

#[spacetimedb::reducer]
pub fn count_unique_0_u32_u64_u64(ctx: &ReducerContext) {
    log::info!("COUNT: {}", ctx.db.unique_0_u32_u64_u64().count());
}

#[spacetimedb::reducer]
pub fn count_no_index_u32_u64_u64(ctx: &ReducerContext) {
    log::info!("COUNT: {}", ctx.db.no_index_u32_u64_u64().count());
}

#[spacetimedb::reducer]
pub fn count_btree_each_column_u32_u64_u64(ctx: &ReducerContext) {
    log::info!("COUNT: {}", ctx.db.btree_each_column_u32_u64_u64().count());
}
// ---------- module-specific stuff ----------

#[spacetimedb::reducer]
pub fn fn_with_1_args(_ctx: &ReducerContext, _arg: String) {}

#[spacetimedb::reducer]
pub fn fn_with_32_args(
    _ctx: &ReducerContext,
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
pub fn print_many_things(_ctx: &ReducerContext, n: u32) {
    for _ in 0..n {
        log::info!("hello again!");
    }
}
