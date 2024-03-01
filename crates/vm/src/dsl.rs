//! Utilities for build valid constructs for the vm.

use crate::expr::{Expr, QueryExpr, SourceExpr};
use crate::relation::MemTable;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::relation::{DbTable, Header};
use std::sync::Arc;

pub fn scalar<T: Into<AlgebraicValue>>(of: T) -> AlgebraicValue {
    of.into()
}

pub fn value<T: Into<AlgebraicValue>>(of: T) -> Expr {
    let v: AlgebraicValue = of.into();
    Expr::Value(v)
}

pub fn mem_table<H, I, T>(head: H, iter: I) -> MemTable
where
    H: Into<Header>,
    I: IntoIterator<Item = T>,
    T: Into<ProductValue>,
{
    MemTable::from_iter(Arc::new(head.into()), iter.into_iter().map(Into::into))
}

pub fn db_table_raw<T: Into<Header>>(
    head: T,
    table_id: TableId,
    table_type: StTableType,
    table_access: StAccess,
) -> DbTable {
    DbTable::new(Arc::new(head.into()), table_id, table_type, table_access)
}

/// Create a [DbTable] of type [StTableType::User] and derive `StAccess::for_name(name)`.
pub fn db_table<T: Into<Header>>(head: T, table_id: TableId) -> DbTable {
    let head = head.into();
    let access = StAccess::for_name(&head.table_name);
    db_table_raw(head, table_id, StTableType::User, access)
}

pub fn query<Source>(source: Source) -> QueryExpr
where
    Source: Into<SourceExpr>,
{
    QueryExpr::new(source)
}
