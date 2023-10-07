//! Utilities for build valid constructs for the vm.
use crate::expr::{Expr, QueryExpr, SourceExpr};
use crate::operator::*;
use spacetimedb_lib::auth::{StAccess, StTableType};
use spacetimedb_lib::relation::{DbTable, Header, MemTable};
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::product_type::ProductType;
use spacetimedb_sats::product_value::ProductValue;
use spacetimedb_sats::SatsString;
use std::collections::HashMap;

pub fn scalar<T: Into<AlgebraicValue>>(of: T) -> AlgebraicValue {
    of.into()
}

pub fn value<T: Into<AlgebraicValue>>(of: T) -> Expr {
    let v: AlgebraicValue = of.into();
    Expr::Value(v)
}

pub fn def<T: Into<Expr>>(name: &str, of: T) -> Expr {
    Expr::Let(Box::new((name.to_string(), of.into())))
}

pub fn var(name: &str) -> Expr {
    Expr::Ident(name.to_string())
}

pub fn mem_table<H, I, T>(head: H, iter: I) -> MemTable
where
    H: Into<Header>,
    I: IntoIterator<Item = T>,
    T: Into<ProductValue>,
{
    MemTable::from_iter(head.into(), iter.into_iter().map(Into::into))
}

pub fn db_table_raw(
    head: ProductType,
    name: SatsString,
    table_id: u32,
    table_type: StTableType,
    table_access: StAccess,
) -> DbTable {
    DbTable::new(
        Header::from_product_type(name, head),
        table_id,
        table_type,
        table_access,
    )
}

/// Create a [DbTable] of type [StTableType::User] and derive `StAccess::for_name(name)`.
pub fn db_table(head: ProductType, name: SatsString, table_id: u32) -> DbTable {
    let access = StAccess::for_name(&name);
    db_table_raw(head, name, table_id, StTableType::User, access)
}

pub fn bin_op<O, A, B>(op: O, a: A, b: B) -> Expr
where
    O: Into<Op>,
    A: Into<Expr>,
    B: Into<Expr>,
{
    Expr::Op(op.into(), vec![a.into(), b.into()])
}

pub fn prefix_op<O, I>(op: O, values: I) -> Expr
where
    O: Into<Op>,
    I: IntoIterator<Item = Expr>,
{
    Expr::Op(op.into(), values.into_iter().collect())
}

pub fn if_<Test, A, B>(check: Test, if_true: A, if_false: B) -> Expr
where
    Test: Into<Expr>,
    A: Into<Expr>,
    B: Into<Expr>,
{
    Expr::If(Box::new((check.into(), if_true.into(), if_false.into())))
}

pub fn params<T: Into<Expr> + Clone>(of: &[(&str, T)]) -> HashMap<String, Expr> {
    let mut p = HashMap::with_capacity(of.len());

    for (k, v) in of {
        p.insert(k.to_string(), v.clone().into());
    }

    p
}

pub fn call_fn<T: Into<Expr> + Clone>(name: &str, with: &[(&str, T)]) -> Expr {
    Expr::CallFn(name.to_string(), params(with))
}

pub fn query<Source>(source: Source) -> QueryExpr
where
    Source: Into<SourceExpr>,
{
    QueryExpr::new(source)
}
