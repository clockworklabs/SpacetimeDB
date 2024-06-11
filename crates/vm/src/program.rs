//! Definition for a `Program` to run code.
//!
//! It carries an [EnvDb] with the functions, idents, types.

use crate::errors::ErrorVm;
use crate::expr::{Code, CrudExpr, SourceSet};
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
