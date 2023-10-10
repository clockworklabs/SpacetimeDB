//! Define an in-progress `type` annotation.
use derive_more::From;
use std::fmt;

use crate::operator::*;
use spacetimedb_sats::algebraic_type::map_notation::fmt_algebraic_type;
use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;

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

pub(crate) fn ty_op(op: Op) -> Vec<Ty> {
    match op {
        Op::Cmp(_) | Op::Logic(_) => vec![AlgebraicType::Bool.into()],
        Op::Unary(x) => match x {
            OpUnary::Not => vec![AlgebraicType::Bool.into()],
        },
        Op::Math(_) => vec![
            AlgebraicType::I8.into(),
            AlgebraicType::U8.into(),
            AlgebraicType::I16.into(),
            AlgebraicType::U16.into(),
            AlgebraicType::I32.into(),
            AlgebraicType::U32.into(),
            AlgebraicType::I32.into(),
            AlgebraicType::U32.into(),
            AlgebraicType::I64.into(),
            AlgebraicType::U64.into(),
            AlgebraicType::I128.into(),
            AlgebraicType::U128.into(),
            AlgebraicType::F32.into(),
            AlgebraicType::F64.into(),
        ],
    }
}
