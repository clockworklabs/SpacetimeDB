use std::marker::PhantomData;

use super::{
    expr::{format_expr, Expr},
    table::{ColumnRef, HasCols, HasIxCols, Table},
    Query, ValueExpr,
};

pub struct IxCol<T, V> {
    pub(super) col: ColumnRef<T>,
    _marker: PhantomData<V>,
}

impl<T, V> IxCol<T, V> {
    pub fn new(name: &'static str) -> Self {
        Self {
            col: ColumnRef::new(name),
            _marker: PhantomData,
        }
    }
}

impl<T, V> Copy for IxCol<T, V> {}
impl<T, V> Clone for IxCol<T, V> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct IxJoinEq<L, R, V> {
    pub(super) lhs_col: ColumnRef<L>,
    pub(super) rhs_col: ColumnRef<R>,
    _marker: PhantomData<V>,
}

impl<T, V> IxCol<T, V> {
    pub fn eq<R: HasIxCols>(self, rhs: IxCol<R, V>) -> IxJoinEq<T, R, V> {
        IxJoinEq {
            lhs_col: self.col,
            rhs_col: rhs.col,
            _marker: PhantomData,
        }
    }
}

pub(super) enum JoinKind {
    Left,
    Right,
}

pub struct JoinWhere<T> {
    pub(super) kind: JoinKind,
    pub(super) left_col: ColumnRef<T>,
    pub(super) right_table: &'static str,
    pub(super) right_col: &'static str,
    pub(super) where_expr: Option<Expr<ValueExpr<T>>>,
}

fn semijoin<L, R, V>(
    lix: L::IxCols,
    rix: R::IxCols,
    on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    where_expr: Option<Expr<ValueExpr<L>>>,
    kind: JoinKind,
) -> JoinWhere<L>
where
    L: HasIxCols,
    R: HasIxCols,
{
    let join = on(&lix, &rix);

    JoinWhere {
        kind,
        left_col: join.lhs_col,
        right_table: R::TABLE_NAME,
        right_col: join.rhs_col.column_name,
        where_expr,
    }
}

impl<L: HasIxCols> Table<L> {
    pub fn left_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> JoinWhere<L> {
        semijoin(L::idx_cols(), R::idx_cols(), on, None, JoinKind::Left)
    }

    pub fn right_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> JoinWhere<L> {
        semijoin(L::idx_cols(), R::idx_cols(), on, None, JoinKind::Right)
    }
}

impl<L: HasIxCols> super::FromWhere<L> {
    pub fn left_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> JoinWhere<L> {
        semijoin(L::idx_cols(), R::idx_cols(), on, Some(self.expr), JoinKind::Left)
    }

    pub fn right_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> JoinWhere<L> {
        semijoin(L::idx_cols(), R::idx_cols(), on, Some(self.expr), JoinKind::Right)
    }
}

impl<T: HasCols> JoinWhere<T> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let extra = f(&T::cols());
        let new = match self.where_expr {
            Some(existing) => Some(existing.and(extra)),
            None => Some(extra),
        };

        Self {
            kind: self.kind,
            left_col: self.left_col,
            right_table: self.right_table,
            right_col: self.right_col,
            where_expr: new,
        }
    }

    pub fn build(self) -> Query {
        let alias = match self.kind {
            JoinKind::Left => "left",
            JoinKind::Right => "right",
        };

        let where_clause = self
            .where_expr
            .map(|e| format!(" WHERE {}", format_expr(&e)))
            .unwrap_or_default();

        let sql = format!(
            r#"SELECT "{}".* FROM "{}" "left" JOIN "{}" "right" ON "left"."{}" = "right"."{}"{}"#,
            alias,
            T::TABLE_NAME,
            self.right_table,
            self.left_col.column_name,
            self.right_col,
            where_clause
        );

        Query { sql }
    }
}
