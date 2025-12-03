use super::{
    expr::{format_expr, BoolExpr},
    table::{ColumnRef, HasCols, HasIxCols, Table},
    Query,
};
use std::marker::PhantomData;

/// Indexed columns for joins
///
/// Joins are performed on indexed columns, Tables that implement `HasIxCols`
/// provide access to their indexed columns.
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

// Left semijoin: filters and returns left table rows
pub struct LeftSemiJoin<L> {
    pub(super) left_col: ColumnRef<L>,
    pub(super) right_table: &'static str,
    pub(super) right_col: &'static str,
    pub(super) where_expr: Option<BoolExpr<L>>,
}

// Right semijoin: returns right table rows, but remembers left conditions
pub struct RightSemiJoin<R, L> {
    pub(super) left_col: ColumnRef<L>,
    pub(super) right_col: ColumnRef<R>,
    pub(super) left_where_expr: Option<BoolExpr<L>>,
    pub(super) right_where_expr: Option<BoolExpr<R>>,
    _left_marker: PhantomData<L>,
}

impl<L: HasIxCols> Table<L> {
    pub fn left_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> LeftSemiJoin<L> {
        let join = on(&L::ix_cols(), &R::ix_cols());
        LeftSemiJoin {
            left_col: join.lhs_col,
            right_table: R::TABLE_NAME,
            right_col: join.rhs_col.column_name(),
            where_expr: None,
        }
    }

    pub fn right_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> RightSemiJoin<R, L> {
        let join = on(&L::ix_cols(), &R::ix_cols());
        RightSemiJoin {
            left_col: join.lhs_col,
            right_col: join.rhs_col,
            left_where_expr: None,
            right_where_expr: None,
            _left_marker: PhantomData,
        }
    }
}

impl<L: HasIxCols> super::FromWhere<L> {
    pub fn left_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> LeftSemiJoin<L> {
        let join = on(&L::ix_cols(), &R::ix_cols());
        LeftSemiJoin {
            left_col: join.lhs_col,
            right_table: R::TABLE_NAME,
            right_col: join.rhs_col.column_name(),
            where_expr: Some(self.expr),
        }
    }

    pub fn right_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> RightSemiJoin<R, L> {
        let join = on(&L::ix_cols(), &R::ix_cols());
        RightSemiJoin {
            left_col: join.lhs_col,
            right_col: join.rhs_col,
            left_where_expr: Some(self.expr),
            right_where_expr: None,
            _left_marker: PhantomData,
        }
    }
}

// LeftSemiJoin where() operates on L
impl<L: HasCols> LeftSemiJoin<L> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&L::Cols) -> BoolExpr<L>,
    {
        let extra = f(&L::cols());
        let new = match self.where_expr {
            Some(existing) => Some(existing.and(extra)),
            None => Some(extra),
        };
        Self {
            left_col: self.left_col,
            right_table: self.right_table,
            right_col: self.right_col,
            where_expr: new,
        }
    }

    pub fn build(self) -> Query<L> {
        let where_clause = self
            .where_expr
            .map(|e| format!(" WHERE {}", format_expr(&e)))
            .unwrap_or_default();

        let sql = format!(
            r#"SELECT "left".* FROM "{}" "left" JOIN "{}" "right" ON "left"."{}" = "right"."{}"{}"#,
            L::TABLE_NAME,
            self.right_table,
            self.left_col.column_name(),
            self.right_col,
            where_clause
        );
        Query::new(sql)
    }
}

// RightSemiJoin where() operates on R
impl<R: HasCols, L: HasCols> RightSemiJoin<R, L> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&R::Cols) -> BoolExpr<R>,
    {
        let extra = f(&R::cols());
        let new = match self.right_where_expr {
            Some(existing) => Some(existing.and(extra)),
            None => Some(extra),
        };
        Self {
            left_col: self.left_col,
            right_col: self.right_col,
            left_where_expr: self.left_where_expr,
            right_where_expr: new,
            _left_marker: PhantomData,
        }
    }

    pub fn build(self) -> Query<R> {
        let mut where_parts = Vec::new();

        if let Some(left_expr) = self.left_where_expr {
            where_parts.push(format_expr(&left_expr));
        }

        if let Some(right_expr) = self.right_where_expr {
            where_parts.push(format_expr(&right_expr));
        }

        let where_clause = if !where_parts.is_empty() {
            format!(" WHERE {}", where_parts.join(" AND "))
        } else {
            String::new()
        };

        let sql = format!(
            r#"SELECT "right".* FROM "{}" "left" JOIN "{}" "right" ON "left"."{}" = "right"."{}"{}"#,
            L::TABLE_NAME,
            R::TABLE_NAME,
            self.left_col.column_name(),
            self.right_col.column_name(),
            where_clause
        );
        Query::new(sql)
    }
}
