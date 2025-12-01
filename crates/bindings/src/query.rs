use spacetimedb_lib::{
    sats::algebraic_value::ser::ValueSerializer, ser::Serialize, AlgebraicType, AlgebraicValue, Identity, SpacetimeType,
};

use crate::table::Column;

pub struct QueryBuilder {}

pub struct Table<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> Table<T> {
    pub fn new() -> Self {
        Table {
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct FromWhere<T> {
    expr: Expr<ValueExpr<T>>,
}

impl<T: HasCols> FromWhere<T> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let cols = T::cols();
        let expr = f(&cols);
        FromWhere {
            expr: expr.and(self.expr),
        }
    }
}

pub struct JoinWhere<T> {
    _marker: std::marker::PhantomData<T>,
}

pub struct Query<T> {
    _marker: std::marker::PhantomData<T>,
}

pub trait HasCols {
    type Cols;
    fn cols() -> Self::Cols;
}

pub struct Col<T, V> {
    column_name: &'static str,
    _marker: std::marker::PhantomData<(T, V)>,
}

impl<T, V> Col<T, V> {
    pub fn new(column_name: &'static str) -> Self {
        Col {
            column_name,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, V> Clone for Col<T, V> {
    fn clone(&self) -> Self {
        Col {
            column_name: self.column_name,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, V> Copy for Col<T, V> {}

impl<T: HasCols> Table<T> {
    pub fn r#where<F>(self, f: F) -> FromWhere<T>
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let cols = T::cols();
        let expr = f(&cols);
        FromWhere { expr }
    }
}

pub trait RHS<T, V> {
    fn to_expr(self) -> ValueExpr<T>;
}

impl<T, V> RHS<T, V> for Col<T, V> {
    fn to_expr(self) -> ValueExpr<T> {
        ValueExpr::Column {
            name: self.column_name,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, V: Serialize> RHS<T, V> for V {
    fn to_expr(self) -> ValueExpr<T> {
        let serializer = ValueSerializer;
        let value = self.serialize(serializer).unwrap();

        ValueExpr::Literal(value.into())
    }
}

impl<T, V: Serialize> Col<T, V> {
    pub fn eq(self, value: impl RHS<T, V>) -> Expr<ValueExpr<T>> {
        Expr::Eq(self.into(), value.to_expr())
    }
}

pub enum ValueExpr<T> {
    Column {
        name: &'static str,
        _marker: std::marker::PhantomData<T>,
    },
    Literal(AlgebraicValue),
}

impl<T, V> From<Col<T, V>> for ValueExpr<T> {
    fn from(col: Col<T, V>) -> Self {
        ValueExpr::Column {
            name: col.column_name,
            _marker: std::marker::PhantomData,
        }
    }
}

pub enum Expr<T> {
    Eq(T, T),
    Neq(T, T),
    Gt(T, T),
    Lt(T, T),
    And(Box<Expr<T>>, Box<Expr<T>>),
}
impl<T> Expr<T> {
    pub fn and(self, other: Expr<T>) -> Expr<T> {
        Expr::And(Box::new(self), Box::new(other))
    }
}
