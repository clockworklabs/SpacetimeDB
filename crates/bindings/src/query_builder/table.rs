use std::marker::PhantomData;

use super::{format_expr, Expr, Query, RHS};
use spacetimedb_lib::{sats::algebraic_value::ser::ValueSerializer, ser::Serialize, AlgebraicValue};

pub trait TableName {
    const TABLE_NAME: &'static str;
}

pub trait HasCols: TableName {
    type Cols;
    fn cols() -> Self::Cols;
}

pub trait HasIxCols: TableName {
    type IxCols;
    fn idx_cols() -> Self::IxCols;
}

pub struct Table<T> {
    _marker: PhantomData<T>,
}

impl<T> Table<T> {
    pub fn new() -> Self {
        Table { _marker: PhantomData }
    }
}

pub struct Col<T, V> {
    pub(super) column_name: &'static str,
    _marker: PhantomData<(T, V)>,
}

impl<T, V> Col<T, V> {
    pub fn new(column_name: &'static str) -> Self {
        Self {
            column_name,
            _marker: PhantomData,
        }
    }
}

impl<T, V> Copy for Col<T, V> {}
impl<T, V> Clone for Col<T, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: TableName, V> Col<T, V> {
    pub fn eq<R: RHS<T, V>>(self, rhs: R) -> Expr<ValueExpr<T>> {
        Expr::Eq(self.into(), rhs.to_expr())
    }
    pub fn neq<R: RHS<T, V>>(self, rhs: R) -> Expr<ValueExpr<T>> {
        Expr::Neq(self.into(), rhs.to_expr())
    }
    pub fn gt<R: RHS<T, V>>(self, rhs: R) -> Expr<ValueExpr<T>> {
        Expr::Gt(self.into(), rhs.to_expr())
    }
    pub fn lt<R: RHS<T, V>>(self, rhs: R) -> Expr<ValueExpr<T>> {
        Expr::Lt(self.into(), rhs.to_expr())
    }
}

pub struct ColumnRef<T> {
    pub(super) column_name: &'static str,
    _marker: PhantomData<T>,
}

impl<T> ColumnRef<T> {
    pub fn new(column_name: &'static str) -> Self {
        Self {
            column_name,
            _marker: PhantomData,
        }
    }

    pub fn fmt(&self) -> String
    where
        T: TableName,
    {
        format!("\"{}\".\"{}\"", T::TABLE_NAME, self.column_name)
    }
}

impl<T> Copy for ColumnRef<T> {}
impl<T> Clone for ColumnRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

pub enum ValueExpr<T> {
    Column(ColumnRef<T>),
    Literal(AlgebraicValue),
}

impl<T: TableName, V> From<Col<T, V>> for ValueExpr<T> {
    fn from(col: Col<T, V>) -> Self {
        ValueExpr::Column(ColumnRef::new(col.column_name))
    }
}

impl<T, V> RHS<T, V> for Col<T, V> {
    fn to_expr(self) -> ValueExpr<T> {
        ValueExpr::Column(ColumnRef::new(self.column_name))
    }
}

impl<T, V: Serialize> RHS<T, V> for V {
    fn to_expr(self) -> ValueExpr<T> {
        let serializer = ValueSerializer;
        let value = self.serialize(serializer).unwrap();
        ValueExpr::Literal(value.into())
    }
}

pub struct FromWhere<T: TableName> {
    pub(super) expr: Expr<ValueExpr<T>>,
}

impl<T: HasCols> Table<T> {
    pub fn build(self) -> Query {
        Query {
            sql: format!(r#"SELECT * FROM "{}""#, T::TABLE_NAME),
        }
    }

    pub fn r#where<F>(self, f: F) -> FromWhere<T>
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let expr = f(&T::cols());
        FromWhere { expr }
    }
}

impl<T: HasCols> FromWhere<T> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let extra = f(&T::cols());
        Self {
            expr: self.expr.and(extra),
        }
    }

    pub fn build(self) -> Query {
        Query {
            sql: format!(r#"SELECT * FROM "{}" WHERE {}"#, T::TABLE_NAME, format_expr(&self.expr)),
        }
    }
}
