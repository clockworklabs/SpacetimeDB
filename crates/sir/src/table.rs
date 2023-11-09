use crate::types::IterRows;
use spacetimedb_lib::auth::StAccess;
use spacetimedb_lib::relation::{Header, RelValue};

/// An table evaluator that resolves on demand functions/expressions
pub struct TableGenerator<'a> {
    pub(crate) evaluate: Option<Box<dyn Fn(&Header, StAccess) -> RelValue>>,
    pub(crate) iter: Box<IterRows<'a>>,
}
