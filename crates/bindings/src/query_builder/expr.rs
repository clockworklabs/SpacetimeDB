use spacetimedb_lib::{
    sats::{i256, u256},
    ConnectionId, Identity,
};

use crate::query_builder::{Col, ColumnRef};

use super::TableName;

pub enum ValueExpr<T> {
    Column(ColumnRef<T>),
    Literal(LiteralValue),
}

pub enum Expr<T> {
    Eq(T, T),
    Neq(T, T),
    Gt(T, T),
    Lt(T, T),
    And(Box<Expr<T>>, Box<Expr<T>>),
    Or(Box<Expr<T>>, Box<Expr<T>>),
}

impl<T> Expr<T> {
    pub fn and(self, other: Expr<T>) -> Expr<T> {
        Expr::And(Box::new(self), Box::new(other))
    }

    pub fn or(self, other: Expr<T>) -> Expr<T> {
        Expr::Or(Box::new(self), Box::new(other))
    }
}

pub trait RHS<T, V> {
    fn to_expr(self) -> ValueExpr<T>;
}

impl<T, V> RHS<T, V> for Col<T, V> {
    fn to_expr(self) -> ValueExpr<T> {
        ValueExpr::Column(ColumnRef::new(self.column_name))
    }
}

fn format_value_expr<T: TableName>(v: &ValueExpr<T>) -> String {
    match v {
        ValueExpr::Column(col) => format!("\"{}\".\"{}\"", T::TABLE_NAME, col.column_name),
        ValueExpr::Literal(lit) => format_literal(lit),
    }
}

pub fn format_expr<T: TableName>(expr: &Expr<ValueExpr<T>>) -> String {
    match expr {
        Expr::Eq(l, r) => format!("({} = {})", format_value_expr(l), format_value_expr(r)),
        Expr::Neq(l, r) => format!("({} <> {})", format_value_expr(l), format_value_expr(r)),
        Expr::Gt(l, r) => format!("({} > {})", format_value_expr(l), format_value_expr(r)),
        Expr::Lt(l, r) => format!("({} < {})", format_value_expr(l), format_value_expr(r)),
        Expr::And(a, b) => format!("({} AND {})", format_expr(a), format_expr(b)),
        Expr::Or(a, b) => format!("({} OR {})", format_expr(a), format_expr(b)),
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
    fn to_expr(self) -> ValueExpr<T> {
        ValueExpr::Literal(LiteralValue::String(self.to_string()))
    }
}

macro_rules! impl_rhs {
    ($ty:ty, $variant:ident $(, $map:expr )? ) => {
        impl<T> RHS<T, $ty> for $ty {
            fn to_expr(self) -> ValueExpr<T> {
                ValueExpr::Literal(
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
