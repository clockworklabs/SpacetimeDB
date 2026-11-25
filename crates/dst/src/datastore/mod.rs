//! DST target for the relational datastore: transactions of inserts and
//! deletes over a generated schema, checked against [`model::Model`], with
//! replays from the commitlog.

mod model;
mod rules;
mod world;

use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::ProductValue;

pub use self::world::Datastore;

pub type Row = ProductValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Accepted(Row),
    UniqueConstraintViolation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDelta {
    pub tables: Vec<TableDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDelta {
    pub table: usize,
    pub inserts: Vec<Row>,
    pub deletes: Vec<Row>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountState {
    pub row_counts: Vec<TableRowCount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableRowCount {
    pub table: usize,
    pub count: u64,
}

pub fn row_to_bytes(row: &Row) -> Vec<u8> {
    to_vec(row).expect("a `ProductValue` always serializes to BSATN")
}

/// Order rows the same way on both sides of a comparison.
pub fn normalize_rows(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by_key(row_to_bytes);
    rows
}
