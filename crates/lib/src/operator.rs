//! Operator support for the query macro.

use derive_more::From;
use spacetimedb_lib::de::Deserialize;
use spacetimedb_lib::ser::Serialize;
use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OpCmp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

impl From<OpCmp> for &str {
    fn from(x: OpCmp) -> Self {
        match x {
            OpCmp::Eq => "std::cmp::eq",
            OpCmp::NotEq => "std::cmp::neq",
            OpCmp::Lt => "std::cmp::lt",
            OpCmp::LtEq => "std::cmp::le",
            OpCmp::Gt => "std::cmp::gt",
            OpCmp::GtEq => "std::cmp::ge",
        }
    }
}

impl OpCmp {
    /// Reverse the order of the `cmp`, to helps in reducing the cases on evaluation, ie:
    pub fn reverse(self) -> Self {
        match self {
            OpCmp::Eq => self,
            OpCmp::NotEq => self,
            OpCmp::Lt => OpCmp::Gt,
            OpCmp::LtEq => OpCmp::GtEq,
            OpCmp::Gt => OpCmp::Lt,
            OpCmp::GtEq => OpCmp::LtEq,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, From)]
pub enum OpQuery {
    Cmp(OpCmp),
    Logic(OpLogic),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, From)]
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

impl fmt::Display for OpCmp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = match self {
            OpCmp::Eq => "==",
            OpCmp::NotEq => "!=",
            OpCmp::Lt => "<",
            OpCmp::LtEq => "<=",
            OpCmp::Gt => ">",
            OpCmp::GtEq => ">=",
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
