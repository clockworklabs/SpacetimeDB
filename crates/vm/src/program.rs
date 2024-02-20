//! Definition for a `Program` to run code.
//!
//! It carries an [EnvDb] with the functions, idents, types.
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Address;
use spacetimedb_sats::relation::{MemTable, RelIter, Relation, Table};

use crate::errors::ErrorVm;
use crate::eval::{build_query, IterRows};
use crate::expr::{Code, CrudCode};
use crate::rel_ops::RelOps;

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
    fn eval_query(&mut self, query: CrudCode) -> Result<Code, ErrorVm>;

    fn as_program_ref(&self) -> ProgramRef<'_>;
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

pub struct ProgramRef<'a> {
    pub ctx: &'a dyn ProgramVm,
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

    #[tracing::instrument(skip_all)]
    fn eval_query(&mut self, query: CrudCode) -> Result<Code, ErrorVm> {
        match query {
            CrudCode::Query(query) => {
                let head = query.head().clone();
                let row_count = query.row_count();
                let table_access = query.table.table_access();
                let result = match query.table {
                    Table::MemTable(x) => Box::new(RelIter::new(head, row_count, x)) as Box<IterRows<'_>>,
                    Table::DbTable(_) => {
                        panic!("DB not set")
                    }
                };

                let result = build_query(result, query.query)?;

                let head = result.head().clone();
                let rows: Vec<_> = result.collect_vec()?;

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

    fn as_program_ref(&self) -> ProgramRef<'_> {
        ProgramRef { ctx: self.ctx() }
    }
}
