//! Operators are implemented as "alias" of functions that are loaded
//! at the start of the [ProgramVm] creation, ie:
//!
//! `+` == std::math::add
//!
use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum OpCmp {
    Eq,
    NotEq,
    Less,
    LessThan,
    Greater,
    GreaterThan,
}

impl From<OpCmp> for &str {
    fn from(x: OpCmp) -> Self {
        match x {
            OpCmp::Eq => "std::cmp::eq",
            OpCmp::NotEq => "std::cmp::neq",
            OpCmp::Less => "std::cmp::le",
            OpCmp::LessThan => "std::cmp::lt",
            OpCmp::Greater => "std::cmp::ge",
            OpCmp::GreaterThan => "std::cmp::gt",
        }
    }
}

impl OpCmp {
    /// Reverse the order of the `cmp`, to helps in reducing the cases on evaluation, ie:
    #[allow(dead_code)]
    pub(crate) fn reverse(self) -> Self {
        match self {
            OpCmp::Eq => self,
            OpCmp::NotEq => self,
            OpCmp::Less => OpCmp::Greater,
            OpCmp::LessThan => OpCmp::GreaterThan,
            OpCmp::Greater => OpCmp::Less,
            OpCmp::GreaterThan => OpCmp::LessThan,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum OpUnary {
    Not,
}

impl From<OpUnary> for &str {
    fn from(x: OpUnary) -> Self {
        match x {
            OpUnary::Not => "std::ops::not",
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum OpMath {
    Add,
    Minus,
    Mul,
    Div,
}

impl From<OpMath> for &str {
    fn from(x: OpMath) -> Self {
        match x {
            OpMath::Add => "std::math::add",
            OpMath::Minus => "std::math::minus",
            OpMath::Mul => "std::math::mul",
            OpMath::Div => "std::math::div",
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum OpLogic {
    And,
    Or,
}

impl From<OpLogic> for &str {
    fn from(x: OpLogic) -> Self {
        match x {
            OpLogic::And => "std::ops::and",
            OpLogic::Or => "std::ops::or",
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum OpQuery {
    Cmp(OpCmp),
    Logic(OpLogic),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Op {
    Cmp(OpCmp),
    Logic(OpLogic),
    Unary(OpUnary),
    Math(OpMath),
}

impl Op {
    pub fn is_logical(&self) -> bool {
        matches!(self, Op::Cmp(_) | Op::Logic(_))
    }
}

impl From<OpCmp> for Op {
    fn from(op: OpCmp) -> Self {
        Op::Cmp(op)
    }
}

impl From<OpCmp> for OpQuery {
    fn from(op: OpCmp) -> Self {
        OpQuery::Cmp(op)
    }
}

impl From<OpLogic> for Op {
    fn from(op: OpLogic) -> Self {
        Op::Logic(op)
    }
}

impl From<OpLogic> for OpQuery {
    fn from(op: OpLogic) -> Self {
        OpQuery::Logic(op)
    }
}

impl From<OpUnary> for Op {
    fn from(op: OpUnary) -> Self {
        Op::Unary(op)
    }
}

impl From<OpMath> for Op {
    fn from(op: OpMath) -> Self {
        Op::Math(op)
    }
}

impl fmt::Display for OpCmp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = match self {
            OpCmp::Eq => "==",
            OpCmp::NotEq => "!=",
            OpCmp::Less => "<",
            OpCmp::LessThan => "<=",
            OpCmp::Greater => ">",
            OpCmp::GreaterThan => ">=",
        };
        write!(f, "{x}")
    }
}

impl fmt::Display for OpLogic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = match self {
            OpLogic::And => "and",
            OpLogic::Or => "or",
        };
        write!(f, "{x}")
    }
}

impl fmt::Display for OpUnary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = match self {
            OpUnary::Not => "not",
        };
        write!(f, "{x}")
    }
}

impl fmt::Display for OpMath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = match self {
            OpMath::Add => "+",
            OpMath::Minus => "-",
            OpMath::Mul => "*",
            OpMath::Div => "/",
        };
        write!(f, "{x}")
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Cmp(x) => {
                write!(f, "{x}")
            }
            Op::Logic(x) => {
                write!(f, "{x}")
            }
            Op::Unary(x) => {
                write!(f, "{x}")
            }
            Op::Math(x) => {
                write!(f, "{x}")
            }
        }
    }
}

impl fmt::Display for OpQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpQuery::Cmp(x) => {
                write!(f, "{x}")
            }
            OpQuery::Logic(x) => {
                write!(f, "{x}")
            }
        }
    }
}
