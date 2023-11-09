use derive_more::From;

use crate::iterator::RelOps;
use spacetimedb_sats::AlgebraicType;

pub type IterRows<'a> = dyn RelOps + 'a;

/// Describe a fully resolved `type`.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, From)]
pub enum Ty {
    #[from]
    Val(AlgebraicType),
    Multi(Vec<Ty>),
    Fun {
        params: Vec<Ty>,
        result: Box<Ty>,
    },
}
