//! Definition for a `Program` to run code.
//!
//! It carries an [EnvDb] with the functions, idents, types.

use crate::errors::ErrorVm;
use crate::eval::{build_query, IterRows};
use crate::expr::{Code, CrudCode, SourceSet};
use crate::iterators::RelIter;
use crate::rel_ops::RelOps;
use crate::relation::MemTable;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Address;
use spacetimedb_sats::relation::Relation;

/// A trait to allow split the execution of `programs` to allow executing
/// `queries` that take in account each `program` state/enviroment.
///
/// To be specific, it allows you to run queries that run on the `SpacetimeDB` engine.
///
/// It could also permit run queries backed by different engines, like in `MySql`.
pub trait ProgramVm {
    fn address(&self) -> Option<Address>;
    fn ctx(&self) -> &dyn ProgramVm;
    fn auth(&self) -> &AuthCtx;

    /// Allows to execute the query with the state carried by the implementation of this
    /// trait
    fn eval_query(&mut self, query: CrudCode, sources: &mut SourceSet) -> Result<Code, ErrorVm>;
}

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
pub struct Program {
    pub(crate) auth: AuthCtx,
}

impl Program {
    pub fn new(auth: AuthCtx) -> Self {
        Self { auth }
    }
}

impl ProgramVm for Program {
    fn address(&self) -> Option<Address> {
        None
    }

    fn ctx(&self) -> &dyn ProgramVm {
        self as &dyn ProgramVm
    }

    fn auth(&self) -> &AuthCtx {
        &self.auth
    }

    fn eval_query(&mut self, query: CrudCode, sources: &mut SourceSet) -> Result<Code, ErrorVm> {
        match query {
            CrudCode::Query(query) => {
                let head = query.head().clone();
                let row_count = query.row_count();
                let table_access = query.source.table_access();
                let result = if let Some(source_id) = query.source.source_id() {
                    let Some(result_table) = sources.take_mem_table(source_id) else {
                        panic!("Query plan specifies a `MemTable` for {source_id:?}, but found a `DbTable` or nothing");
                    };
                    Box::new(RelIter::new(head, row_count, result_table)) as Box<IterRows<'_>>
                } else {
                    panic!("DB not set")
                };

                let result = build_query(result, &query.query, sources)?;

                let head = result.head().clone();
                let rows: Vec<_> = result.collect_vec(|row| row.into_product_value())?;

                Ok(Code::Table(MemTable::new(head, table_access, rows)))
            }
            CrudCode::Insert { .. } => {
                todo!()
            }
            CrudCode::Update { .. } => {
                todo!()
            }
            CrudCode::Delete { .. } => {
                todo!()
            }
            CrudCode::CreateTable { .. } => {
                todo!()
            }
            CrudCode::Drop { .. } => {
                todo!()
            }
        }
    }
}
