use derive_more::From;
use std::fmt;

use spacetimedb_sats::algebraic_type::map_notation::fmt_algebraic_type;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};

/// Describe a `type`. In the case of [Ty::Unknown] the type of [Expr] is
/// not yet know and should be resolved by the type-checker.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, From)]
pub enum Ty {
    Unknown,
    #[from]
    Val(AlgebraicType),
    Multi(Vec<Ty>),
    Fun {
        params: Vec<Ty>,
        result: Box<Ty>,
    },
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Unknown => write!(f, "{self:?}"),
            Ty::Val(ty) => write!(f, "{}", fmt_algebraic_type(ty)),
            Ty::Multi(options) => options.iter().try_for_each(|x| write!(f, "{x}")),
            Ty::Fun { params, result } => {
                write!(f, "(")?;
                for (pos, x) in params.iter().enumerate() {
                    write!(f, "{x}")?;
                    if pos + 1 < params.len() {
                        write!(f, ", ")?;
                    }
                }

                write!(f, ") -> {result}")
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TyExpr<T> {
    pub(crate) of: T,
    pub(crate) ty: Ty,
}

pub trait TypeOf {
    fn type_of(&self) -> Ty;
}

impl TypeOf for AlgebraicType {
    fn type_of(&self) -> Ty {
        Ty::Val(self.clone())
    }
}

impl TypeOf for AlgebraicValue {
    fn type_of(&self) -> Ty {
        Ty::Val(self.type_of())
    }
}
