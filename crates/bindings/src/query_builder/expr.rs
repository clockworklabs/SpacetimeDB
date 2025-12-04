use spacetimedb_lib::{
    sats::{i256, u256},
    ConnectionId, Identity, Timestamp,
};

use crate::query_builder::{Col, ColumnRef};

pub enum Operand<T> {
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

/// Trait for types that can be used as the right-hand side of a comparison with a column of type V
/// in table T.
///
/// This trait is implemented for Col<T, V> and various literal types.
pub trait RHS<T, V> {
    fn to_expr(self) -> Operand<T>;
}

impl<T, V> RHS<T, V> for Col<T, V> {
    fn to_expr(self) -> Operand<T> {
        Operand::Column(self.col)
    }
}

fn format_bool_expr<T>(v: &Operand<T>) -> String {
    match v {
        Operand::Column(col) => col.fmt(),
        Operand::Literal(lit) => lit.0.clone(),
    }
}

pub fn format_expr<T>(expr: &BoolExpr<T>) -> String {
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
pub struct LiteralValue(String);

impl LiteralValue {
    pub fn new(s: String) -> Self {
        Self(s)
    }
}

macro_rules! impl_rhs {
    ($ty:ty, $formatter:expr) => {
        impl<T> RHS<T, $ty> for $ty {
            fn to_expr(self) -> Operand<T> {
                Operand::Literal(LiteralValue($formatter(self)))
            }
        }
    };
}

impl_rhs!(String, |v: String| format!("'{}'", v.replace('\'', "''")));
impl_rhs!(&str, |v: &str| format!("'{}'", v.replace('\'', "''")));

impl_rhs!(i8, |v: i8| v.to_string());
impl_rhs!(i16, |v: i16| v.to_string());
impl_rhs!(i32, |v: i32| v.to_string());
impl_rhs!(i64, |v: i64| v.to_string());
impl_rhs!(i128, |v: i128| v.to_string());

impl_rhs!(u8, |v: u8| v.to_string());
impl_rhs!(u16, |v: u16| v.to_string());
impl_rhs!(u32, |v: u32| v.to_string());
impl_rhs!(u64, |v: u64| v.to_string());
impl_rhs!(u128, |v: u128| v.to_string());
impl_rhs!(usize, |v: usize| v.to_string());

impl_rhs!(u256, |v: u256| v.to_string());
impl_rhs!(i256, |v: i256| v.to_string());

impl_rhs!(f32, |v: f32| (v as f64).to_string());
impl_rhs!(f64, |v: f64| v.to_string());

impl_rhs!(bool, |b: bool| if b { "TRUE".into() } else { "FALSE".into() });

impl_rhs!(Identity, |id: Identity| format!("0x{}", id.to_hex()));
impl_rhs!(ConnectionId, |id: ConnectionId| format!("0x{}", id.to_hex()));
impl_rhs!(Timestamp, |ts: Timestamp| format!("'{}'", ts));

impl_rhs!(Vec<u8>, |b: Vec<u8>| {
    let hex: String = b.iter().map(|x| format!("{:02x}", x)).collect();
    format!("0x{}", hex)
});
