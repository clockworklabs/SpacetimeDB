use spacetimedb_lib::{
    sats::algebraic_value::ser::ValueSerializer, ser::Serialize, AlgebraicType, AlgebraicValue, Identity, SpacetimeType,
};

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

struct ColExpr<T> {
    name: &'static str,
    _marker: std::marker::PhantomData<T>,
}
pub enum ValueExprType<T, V> {
    Column(ColExpr<T>),
    Literal(AlgebraicValue),
    Parameter(V),
}

impl<T, V: Serialize> From<ValueExprType<T, V>> for ValueExpr<T> {
    fn from(value: ValueExprType<T, V>) -> Self {
        match value {
            ValueExprType::Column(col_expr) => ValueExpr::Column(col_expr),
            ValueExprType::Literal(v) => {
                let serializer = ValueSerializer;
                let value = v.serialize(serializer).unwrap();
                ValueExpr::Literal(value)
            }
            _ => {
                panic!("Parameters are not supported in this context");
            }
        }
    }
}

impl<T, V: Serialize> Col<T, V> {
    pub fn eq(self, value: ValueExprType<T, V>) -> Expr<ValueExpr<T>> {
        Expr::Eq(self.into(), value.into())
    }
}

pub enum ValueExpr<T> {
    Column(ColExpr<T>),
    Literal(AlgebraicValue),
}

impl<T, V> From<Col<T, V>> for ValueExprType<T, V> {
    fn from(col: Col<T, V>) -> Self {
        ValueExprType::Column(ColExpr {
            name: col.column_name,
            _marker: std::marker::PhantomData,
        })
    }
}

impl<T, V> From<Col<T, V>> for ValueExpr<T> {
    fn from(col: Col<T, V>) -> Self {
        ValueExpr::Column(ColExpr {
            name: col.column_name,
            _marker: std::marker::PhantomData,
        })
    }
}

impl<T, V: Serialize> From<V> for ValueExprType<T, V> {
    fn from(value: V) -> Self {
        let serializer = ValueSerializer;
        let value = value.serialize(serializer).unwrap();

        ValueExprType::Literal(value.into())
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
