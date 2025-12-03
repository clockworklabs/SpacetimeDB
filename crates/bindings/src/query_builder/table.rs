use std::marker::PhantomData;

use crate::query_builder::Operand;

use super::{format_expr, BoolExpr, Query, RHS};
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
    fn ix_cols() -> Self::IxCols;
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
    pub fn eq<R: RHS<T, V>>(self, rhs: R) -> BoolExpr<T> {
        BoolExpr::Eq(self.into(), rhs.to_expr())
    }
    pub fn ne<R: RHS<T, V>>(self, rhs: R) -> BoolExpr<T> {
        BoolExpr::Ne(self.into(), rhs.to_expr())
    }
    pub fn gt<R: RHS<T, V>>(self, rhs: R) -> BoolExpr<T> {
        BoolExpr::Gt(self.into(), rhs.to_expr())
    }
    pub fn lt<R: RHS<T, V>>(self, rhs: R) -> BoolExpr<T> {
        BoolExpr::Lt(self.into(), rhs.to_expr())
    }
}

impl<T: TableName, V> From<Col<T, V>> for Operand<T> {
    fn from(col: Col<T, V>) -> Self {
        Operand::Column(ColumnRef::new(col.column_name))
    }
}

pub(super) struct ColumnRef<T> {
    column_name: &'static str,
    _marker: PhantomData<T>,
}

impl<T> ColumnRef<T> {
    pub(super) fn new(column_name: &'static str) -> Self {
        Self {
            column_name,
            _marker: PhantomData,
        }
    }

    pub(super) fn fmt(&self) -> String
    where
        T: TableName,
    {
        format!("\"{}\".\"{}\"", T::TABLE_NAME, self.column_name)
    }

    pub(super) fn column_name(&self) -> &'static str {
        self.column_name
    }
}

impl<T> Copy for ColumnRef<T> {}
impl<T> Clone for ColumnRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct FromWhere<T: TableName> {
    pub(super) expr: BoolExpr<T>,
}

impl<T: HasCols> Table<T> {
    pub fn build(self) -> Query<T> {
        let sql = format!(r#"SELECT * FROM "{}""#, T::TABLE_NAME);
        Query::new(sql)
    }

    pub fn r#where<F>(self, f: F) -> FromWhere<T>
    where
        F: Fn(&T::Cols) -> BoolExpr<T>,
    {
        let expr = f(&T::cols());
        FromWhere { expr }
    }
}

impl<T: HasCols> FromWhere<T> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> BoolExpr<T>,
    {
        let extra = f(&T::cols());
        Self {
            expr: self.expr.and(extra),
        }
    }

    pub fn build(self) -> Query<T> {
        let sql = format!(r#"SELECT * FROM "{}" WHERE {}"#, T::TABLE_NAME, format_expr(&self.expr));
        Query::new(sql)
    }
}
