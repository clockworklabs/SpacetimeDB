use spacetimedb_lib::sats::satn::Satn as _;

use super::table::ValueExpr;
use super::TableName;

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

fn format_value_expr<T: TableName>(v: &ValueExpr<T>) -> String {
    match v {
        ValueExpr::Column(col) => format!("\"{}\".\"{}\"", T::TABLE_NAME, col.column_name),
        ValueExpr::Literal(av) => format_literal(av),
    }
}

fn format_literal(v: &spacetimedb_lib::AlgebraicValue) -> String {
    match v {
        spacetimedb_lib::AlgebraicValue::String(s) => format!("'{}'", s.replace("'", "''")),
        _ => v.to_satn(),
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
