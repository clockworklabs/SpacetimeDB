use std::marker::PhantomData;

use crate::Operand;

use super::{format_expr, BoolExpr, Query, RHS};

pub type TableNameStr = &'static str;

pub trait HasCols {
    type Cols;
    fn cols(name: TableNameStr) -> Self::Cols;
}

pub trait HasIxCols {
    type IxCols;
    fn ix_cols(name: TableNameStr) -> Self::IxCols;
}

pub struct Table<T> {
    pub(super) table_name: TableNameStr,
    _marker: PhantomData<T>,
}

impl<T> Table<T> {
    pub fn new(table_name: TableNameStr) -> Self {
        Self {
            table_name,
            _marker: PhantomData,
        }
    }

    pub(super) fn name(&self) -> TableNameStr {
        self.table_name
    }
}

/// Represents a column of type V in table T.
pub struct Col<T, V> {
    pub(super) col: ColumnRef<T>,
    _marker: PhantomData<V>,
}

impl<T, V> Col<T, V> {
    pub fn new(table_name: &'static str, column_name: &'static str) -> Self {
        Self {
            col: ColumnRef::new(table_name, column_name),
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

impl<T, V> Col<T, V> {
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
    pub fn gte<R: RHS<T, V>>(self, rhs: R) -> BoolExpr<T> {
        BoolExpr::Gte(self.into(), rhs.to_expr())
    }
    pub fn lte<R: RHS<T, V>>(self, rhs: R) -> BoolExpr<T> {
        BoolExpr::Lte(self.into(), rhs.to_expr())
    }
}

impl<T, V> From<Col<T, V>> for Operand<T> {
    fn from(col: Col<T, V>) -> Self {
        Operand::Column(col.col)
    }
}

pub struct ColumnRef<T> {
    table_name: &'static str,
    column_name: &'static str,
    _marker: PhantomData<T>,
}

impl<T> ColumnRef<T> {
    pub(super) fn new(table_name: &'static str, column_name: &'static str) -> Self {
        Self {
            table_name,
            column_name,
            _marker: PhantomData,
        }
    }

    pub(super) fn fmt(&self) -> String {
        format!("\"{}\".\"{}\"", self.table_name, self.column_name)
    }

    pub(super) fn column_name(&self) -> &'static str {
        self.column_name
    }

    pub(super) fn table_name(&self) -> &'static str {
        self.table_name
    }
}

impl<T> Copy for ColumnRef<T> {}
impl<T> Clone for ColumnRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct FromWhere<T> {
    pub(super) table_name: TableNameStr,
    pub(super) expr: BoolExpr<T>,
}

impl<T: HasCols> Table<T> {
    pub fn build(self) -> Query<T> {
        let sql = format!(r#"SELECT * FROM "{}""#, self.table_name);
        Query::new(sql)
    }

    pub fn r#where<F>(self, f: F) -> FromWhere<T>
    where
        F: Fn(&T::Cols) -> BoolExpr<T>,
    {
        let expr = f(&T::cols(self.table_name));
        FromWhere {
            table_name: self.table_name,
            expr,
        }
    }

    // Filter is an alias for where
    pub fn filter<F>(self, f: F) -> FromWhere<T>
    where
        F: Fn(&T::Cols) -> BoolExpr<T>,
    {
        self.r#where(f)
    }
}

impl<T: HasCols> FromWhere<T> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> BoolExpr<T>,
    {
        let extra = f(&T::cols(self.table_name));
        Self {
            table_name: self.table_name,
            expr: self.expr.and(extra),
        }
    }

    // Filter is an alias for where
    pub fn filter<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> BoolExpr<T>,
    {
        self.r#where(f)
    }

    pub fn build(self) -> Query<T> {
        let sql = format!(
            r#"SELECT * FROM "{}" WHERE {}"#,
            self.table_name,
            format_expr(&self.expr)
        );
        Query::new(sql)
    }
}
