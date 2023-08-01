//! Define an in-progress `type` annotation.
use std::fmt;

use crate::operator::*;
use spacetimedb_sats::algebraic_type::map_notation::fmt_algebraic_type;
use spacetimedb_sats::algebraic_type::AlgebraicType;
use spacetimedb_sats::algebraic_value::AlgebraicValue;
use spacetimedb_sats::builtin_type::BuiltinType;

/// Describe a `type`. In the case of [Ty::Unknown] the type of [Expr] is
/// not yet know and should be resolved by the type-checker.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Ty {
    Unknown,
    Val(AlgebraicType),
    Multi(Vec<Ty>),
    Fun { params: Vec<Ty>, result: Box<Ty> },
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

impl From<AlgebraicType> for Ty {
    fn from(x: AlgebraicType) -> Self {
        Ty::Val(x)
    }
}

impl From<BuiltinType> for Ty {
    fn from(x: BuiltinType) -> Self {
        Ty::Val(x.into())
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
        Op::Cmp(_) | Op::Logic(_) => vec![BuiltinType::Bool.into()],
        Op::Unary(x) => match x {
            OpUnary::Not => vec![BuiltinType::Bool.into()],
        },
        Op::Math(_) => vec![
            BuiltinType::I8.into(),
            BuiltinType::U8.into(),
            BuiltinType::I16.into(),
            BuiltinType::U16.into(),
            BuiltinType::I32.into(),
            BuiltinType::U32.into(),
            BuiltinType::I32.into(),
            BuiltinType::U32.into(),
            BuiltinType::I64.into(),
            BuiltinType::U64.into(),
            BuiltinType::I128.into(),
            BuiltinType::U128.into(),
            BuiltinType::F32.into(),
            BuiltinType::F64.into(),
        ],
    }
}
