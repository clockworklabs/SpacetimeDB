//! Definition for a `Program` to run code.
//!
//! It carries an [EnvDb] with the functions, idents, types.

use crate::errors::ErrorVm;
use crate::eval::{build_query, build_source_expr_query};
use crate::expr::{Code, CrudExpr, SourceSet};
use crate::rel_ops::RelOps;
use crate::relation::MemTable;
use spacetimedb_sats::ProductValue;

/// A trait to allow split the execution of `programs` to allow executing
/// `queries` that take in account each `program` state/enviroment.
///
/// To be specific, it allows you to run queries that run on the `SpacetimeDB` engine.
///
/// It could also permit run queries backed by different engines, like in `MySql`.
pub trait ProgramVm {
    /// Allows to execute the query with the state carried by the implementation of this
    /// trait
    fn eval_query<const N: usize>(&mut self, query: CrudExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm>;
}

pub type Sources<'a, const N: usize> = &'a mut SourceSet<Vec<ProductValue>, N>;

/// A default program that run in-memory without a database
pub struct Program;

impl ProgramVm for Program {
    fn eval_query<const N: usize>(&mut self, query: CrudExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm> {
        match query {
            CrudExpr::Query(query) => {
                let result = build_source_expr_query(sources, &query.source);
                let result = build_query(result, &query.query, sources)?;

                let head = result.head().clone();
                let rows: Vec<_> = result.collect_vec(|row| row.into_product_value());

                Ok(Code::Table(MemTable::new(head, query.source.table_access(), rows)))
            }
            CrudExpr::Insert { .. } => {
                todo!()
            }
            CrudExpr::Update { .. } => {
                todo!()
            }
            CrudExpr::Delete { .. } => {
                todo!()
            }
            CrudExpr::CreateTable { .. } => {
                todo!()
            }
            CrudExpr::Drop { .. } => {
                todo!()
            }
            CrudExpr::SetVar { .. } => {
                todo!()
            }
            CrudExpr::ReadVar { .. } => {
                todo!()
            }
        }
    }
}
