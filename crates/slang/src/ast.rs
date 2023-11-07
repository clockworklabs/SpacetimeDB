use derive_more::From;
use spacetimedb_lib::operator::Op;
use std::collections::HashMap;

use spacetimedb_sats::{AlgebraicType, AlgebraicValue};

#[derive(Debug, Clone, Eq, PartialEq, From)]
pub enum Expr {
    #[from]
    Value(AlgebraicValue),
    #[from]
    Ty(AlgebraicType),
    Op(Op, Vec<Expr>),
    //Fun(Function),
    Block(Vec<Expr>),
    CallFn(String, HashMap<String, Expr>),
    Param(Box<(String, Expr)>),
    Let(Box<(String, Expr)>),
    Ident(String),
    If(Box<(Expr, Expr, Expr)>),
    //Crud(Box<CrudExpr>),
}
