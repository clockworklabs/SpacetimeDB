pub struct QueryBuilder {}

pub struct Table<T> {
    _marker: std::marker::PhantomData<T>,
}

pub struct FromWhere<T> {
    _marker: std::marker::PhantomData<T>,
}

pub struct JoinWhere<T> {
    _marker: std::marker::PhantomData<T>,
}

pub struct Query<T> {
    _marker: std::marker::PhantomData<T>,
}

trait HasCols {}
trait HasIxCols {}

type Col<T, U> = (T, U);

impl<T: HasCols> Table<T> {
    pub fn r#where<F>(self, f: F) -> FromWhere<T>
//where
//        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        FromWhere {
            _marker: std::marker::PhantomData,
        }
    }
}

struct ValueExpr<T> {
    _marker: std::marker::PhantomData<T>,
}
enum Expr<T> {
    Eq(T, T),
    Neq(T, T),
    Gt(T, T),
    Lt(T, T),
}
