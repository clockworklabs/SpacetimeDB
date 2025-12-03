use spacetimedb_lib::{
    sats::{i256, u256},
    ConnectionId, Identity,
};

use crate::query_builder::{Col, ColumnRef};

use super::TableName;

pub(super) enum Operand<T> {
    Column(ColumnRef<T>),
    Literal(LiteralValue),
}

pub enum BoolExpr<T> {
    Eq(Operand<T>, Operand<T>),
    Ne(Operand<T>, Operand<T>),
    Gt(Operand<T>, Operand<T>),
    Lt(Operand<T>, Operand<T>),
    And(Box<BoolExpr<T>>, Box<BoolExpr<T>>),
    Or(Box<BoolExpr<T>>, Box<BoolExpr<T>>),
}

impl<T> BoolExpr<T> {
    pub fn and(self, other: BoolExpr<T>) -> BoolExpr<T> {
        BoolExpr::And(Box::new(self), Box::new(other))
    }

    pub fn or(self, other: BoolExpr<T>) -> BoolExpr<T> {
        BoolExpr::Or(Box::new(self), Box::new(other))
    }
}

pub trait RHS<T, V> {
    fn to_expr(self) -> Operand<T>;
}

impl<T, V> RHS<T, V> for Col<T, V> {
    fn to_expr(self) -> Operand<T> {
        Operand::Column(ColumnRef::new(self.column_name))
    }
}

fn format_bool_expr<T: TableName>(v: &Operand<T>) -> String {
    match v {
        Operand::Column(col) => col.fmt(),
        Operand::Literal(lit) => format_literal(lit),
    }
}

pub fn format_expr<T: TableName>(expr: &BoolExpr<T>) -> String {
    match expr {
        BoolExpr::Eq(l, r) => format!("({} = {})", format_bool_expr(l), format_bool_expr(r)),
        BoolExpr::Ne(l, r) => format!("({} <> {})", format_bool_expr(l), format_bool_expr(r)),
        BoolExpr::Gt(l, r) => format!("({} > {})", format_bool_expr(l), format_bool_expr(r)),
        BoolExpr::Lt(l, r) => format!("({} < {})", format_bool_expr(l), format_bool_expr(r)),
        BoolExpr::And(a, b) => format!("({} AND {})", format_expr(a), format_expr(b)),
        BoolExpr::Or(a, b) => format!("({} OR {})", format_expr(a), format_expr(b)),
    }
}

#[derive(Clone, Debug)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Float(f64),
    BigInt(String),
    Bool(bool),
    Identity(Identity),
    ConnectionId(ConnectionId),
}

pub fn format_literal(l: &LiteralValue) -> String {
    match l {
        LiteralValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        LiteralValue::Int(v) => v.to_string(),
        LiteralValue::Float(v) => v.to_string(),
        LiteralValue::Bool(b) => {
            if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        LiteralValue::Identity(identity) => format!("0x{}", identity.to_hex()),
        LiteralValue::ConnectionId(connection) => format!("0x{}", connection.to_hex()),
        LiteralValue::BigInt(bi) => bi.to_string(),
    }
}

impl<T> RHS<T, &str> for &str {
    fn to_expr(self) -> Operand<T> {
        Operand::Literal(LiteralValue::String(self.to_string()))
    }
}

macro_rules! impl_rhs {
    ($ty:ty, $variant:ident $(, $map:expr )? ) => {
        impl<T> RHS<T, $ty> for $ty {
            fn to_expr(self) -> Operand<T> {
                Operand::Literal(
                    LiteralValue::$variant(
                        impl_rhs!(@map self $(, $map )? )
                    )
                )
            }
        }
    };

    (@map $value:expr, $map:expr) => { $map($value) };
    (@map $value:expr) => { $value };
}

impl_rhs!(String, String);

// Integers → Int(i64)
impl_rhs!(i8, Int, |v: i8| v as i64);
impl_rhs!(i16, Int, |v: i16| v as i64);
impl_rhs!(i32, Int, |v: i32| v as i64);
impl_rhs!(i64, Int);

impl_rhs!(u8, Int, |v: u8| v as i64);
impl_rhs!(u16, Int, |v: u16| v as i64);
impl_rhs!(u32, Int, |v: u32| v as i64);

// Big integers → BigInt
impl_rhs!(i128, BigInt, |v: i128| v.to_string());
impl_rhs!(u64, BigInt, |v: u64| v.to_string());
impl_rhs!(usize, BigInt, |v: usize| v.to_string());
impl_rhs!(u128, BigInt, |v: u128| v.to_string());
impl_rhs!(u256, BigInt, |v: u256| v.to_string());
impl_rhs!(i256, BigInt, |v: i256| v.to_string());

impl_rhs!(f32, Float, |v: f32| v as f64);
impl_rhs!(f64, Float);
impl_rhs!(bool, Bool);

impl_rhs!(Identity, Identity);
impl_rhs!(ConnectionId, ConnectionId);
