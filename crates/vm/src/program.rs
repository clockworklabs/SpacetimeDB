//! Definition for a `Program` to run code.
//!
//! It carries an [EnvDb] with the functions, idents, types.

use crate::errors::ErrorVm;
use crate::eval::{build_query, IterRows};
use crate::expr::{Code, CrudExpr, SourceSet};
use crate::iterators::RelIter;
use crate::rel_ops::RelOps;
use crate::relation::{MemTable, RelValue};
use spacetimedb_sats::relation::Relation;

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

pub type Sources<'a, const N: usize> = &'a mut SourceSet<MemTable, N>;

pub struct ProgramStore<P> {
    pub p: P,
    pub code: Code,
}

impl<P> ProgramStore<P> {
    pub fn new(p: P, code: Code) -> Self {
        Self { p, code }
    }
}

/// A default program that run in-memory without a database
pub struct Program;

impl ProgramVm for Program {
    fn eval_query<const N: usize>(&mut self, query: CrudExpr, sources: Sources<'_, N>) -> Result<Code, ErrorVm> {
        match query {
            CrudExpr::Query(query) => {
                let head = query.head().clone();
                let row_count = query.row_count();
                let table_access = query.source.table_access();
                let result = if let Some(source_id) = query.source.source_id() {
                    let Some(result_table) = sources.take(source_id) else {
                        panic!("Query plan specifies a `MemTable` for {source_id:?}, but found a `DbTable` or nothing");
                    };
                    let iter = result_table.data.into_iter().map(RelValue::Projection);
                    Box::new(RelIter::new(head, row_count, iter)) as Box<IterRows<'_>>
                } else {
                    panic!("DB not set")
                };

                let result = build_query(result, &query.query, sources)?;

                let head = result.head().clone();
                let rows: Vec<_> = result.collect_vec(|row| row.into_product_value())?;

                Ok(Code::Table(MemTable::new(head, table_access, rows)))
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
        }
    }
}
