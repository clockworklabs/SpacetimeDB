use std::marker::PhantomData;

use spacetimedb_lib::{
    sats::{algebraic_value::ser::ValueSerializer, satn::Satn},
    ser::Serialize,
    AlgebraicValue,
};

pub struct QueryBuilder {}

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

pub struct Query {
    sql: String,
}

impl Query {
    pub fn sql(&self) -> &str {
        &self.sql
    }
}

pub struct Table<T> {
    _marker: PhantomData<T>,
}

impl<T> Table<T> {
    pub fn new() -> Self {
        Table { _marker: PhantomData }
    }
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
        let cols = T::cols();
        let expr = f(&cols);
        FromWhere { expr }
    }
}

pub struct IxJoinEq<L, R, V> {
    lhs_col: ColumnRef<L>,
    rhs_col: ColumnRef<R>,
    _marker: PhantomData<V>,
}

fn semijoin<L, R, V>(
    lix: L::IxCols,
    rix: R::IxCols,
    on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    where_expr: Option<Expr<ValueExpr<L>>>,
    kind: JoinKind,
) -> JoinWhere<L>
where
    R: HasIxCols,
    L: HasIxCols,
{
    let join = on(&lix, &rix);
    JoinWhere::new(join, where_expr, kind)
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

pub struct Col<T, V> {
    pub(crate) column_name: &'static str,
    _marker: PhantomData<(T, V)>,
}

impl<T, V> Col<T, V> {
    pub fn new(column_name: &'static str) -> Self {
        Col {
            column_name,
            _marker: PhantomData,
        }
    }
}

impl<T, V> Clone for Col<T, V> {
    fn clone(&self) -> Self {
        Col::new(self.column_name)
    }
}
impl<T, V> Copy for Col<T, V> {}

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

pub struct IxCol<T, V> {
    col: ColumnRef<T>,
    _marker: PhantomData<V>,
}

impl<T, V> IxCol<T, V> {
    pub fn new(column_name: &'static str) -> Self {
        IxCol {
            col: ColumnRef {
                column_name,
                _marker: PhantomData,
            },
            _marker: PhantomData,
        }
    }
}

impl<T, V> Clone for IxCol<T, V> {
    fn clone(&self) -> Self {
        IxCol::new(self.col.column_name)
    }
}
impl<T, V> Copy for IxCol<T, V> {}

impl<T, V> IxCol<T, V> {
    pub fn eq<R: HasIxCols>(self, rhs: IxCol<R, V>) -> IxJoinEq<T, R, V> {
        IxJoinEq {
            lhs_col: self.col,
            rhs_col: rhs.col,
            _marker: PhantomData,
        }
    }
}

pub struct FromWhere<T: TableName> {
    expr: Expr<ValueExpr<T>>,
}

impl<T: HasCols> FromWhere<T> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let extra = f(&T::cols());
        FromWhere {
            expr: self.expr.and(extra),
        }
    }

    pub fn build(self) -> Query {
        let where_clause = format_expr(&self.expr);

        Query {
            sql: format!(r#"SELECT * FROM "{}" WHERE {}"#, T::TABLE_NAME, where_clause),
        }
    }
}

impl<L: HasIxCols> FromWhere<L> {
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

/// After a join.
pub struct JoinWhere<T> {
    kind: JoinKind,
    col: ColumnRef<T>,
    right_table: &'static str,
    right_col: &'static str,
    where_expr: Option<Expr<ValueExpr<T>>>,
}

enum JoinKind {
    Left,
    Right,
}

impl<T: TableName> JoinWhere<T> {
    fn new<R, V>(join: IxJoinEq<T, R, V>, where_expr: Option<Expr<ValueExpr<T>>>, kind: JoinKind) -> Self
    where
        R: TableName,
    {
        JoinWhere {
            kind,
            col: join.lhs_col,
            right_table: R::TABLE_NAME,
            right_col: join.rhs_col.column_name,
            where_expr,
        }
    }
}

impl<T: HasCols> JoinWhere<T> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let extra = f(&T::cols());
        let where_expr = match self.where_expr {
            Some(existing) => Some(existing.and(extra)),
            None => Some(extra),
        };

        JoinWhere {
            kind: self.kind,
            col: self.col,
            right_table: self.right_table,
            right_col: self.right_col,
            where_expr,
        }
    }

    pub fn build(self) -> Query {
        let JoinWhere {
            kind,
            col: left_col_ref,
            right_table,
            right_col,
            where_expr,
        } = self;

        let select_side = match kind {
            JoinKind::Left => "left",
            JoinKind::Right => "right",
        };
        let where_clause = where_expr
            .map(|expr| format!(" WHERE {}", format_expr(&expr)))
            .unwrap_or_default();

        let sql = format!(
            r#"SELECT "{}".* FROM "{}" "left" JOIN "{}" "right" ON "left"."{}" = "right"."{}"{}"#,
            select_side,
            T::TABLE_NAME,
            right_table,
            left_col_ref.column_name,
            right_col,
            where_clause
        );

        Query { sql }
    }
}

/// RHS of a comparison
pub trait RHS<T, V> {
    fn to_expr(self) -> ValueExpr<T>;
}

impl<T, V> RHS<T, V> for Col<T, V> {
    fn to_expr(self) -> ValueExpr<T> {
        ValueExpr::Column(ColumnRef {
            column_name: self.column_name,
            _marker: PhantomData,
        })
    }
}

impl<T, V: Serialize> RHS<T, V> for V {
    fn to_expr(self) -> ValueExpr<T> {
        let serializer = ValueSerializer;
        let value = self.serialize(serializer).unwrap();
        ValueExpr::Literal(value.into())
    }
}

struct ColumnRef<T> {
    column_name: &'static str,
    _marker: PhantomData<T>,
}

impl<T> ColumnRef<T> {
    fn new(column_name: &'static str) -> Self {
        ColumnRef {
            column_name,
            _marker: PhantomData,
        }
    }
}

impl<T> Clone for ColumnRef<T> {
    fn clone(&self) -> Self {
        ColumnRef::new(self.column_name)
    }
}
impl<T> Copy for ColumnRef<T> {}

enum ValueExpr<T> {
    Column(ColumnRef<T>),
    Literal(AlgebraicValue),
}

impl<T: TableName, V> From<Col<T, V>> for ValueExpr<T> {
    fn from(col: Col<T, V>) -> Self {
        ValueExpr::Column(ColumnRef::new(col.column_name))
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

pub fn format_expr<T: TableName>(expr: &Expr<ValueExpr<T>>) -> String {
    match expr {
        Expr::Eq(l, r) => format!("({} = {})", format_value_expr(l), format_value_expr(r)),
        Expr::Neq(l, r) => format!("({} <> {})", format_value_expr(l), format_value_expr(r)),
        Expr::Gt(l, r) => format!("({} > {})", format_value_expr(l), format_value_expr(r)),
        Expr::Lt(l, r) => format!("({} < {})", format_value_expr(l), format_value_expr(r)),
        Expr::And(a, b) => format!("({} AND {})", format_expr(a), format_expr(b)),
    }
}

fn format_value_expr<T: TableName>(v: &ValueExpr<T>) -> String {
    match v {
        ValueExpr::Column(ColumnRef { column_name, .. }) => format!("\"{}\".\"{}\"", T::TABLE_NAME, column_name),

        ValueExpr::Literal(av) => format_literal(av),
    }
}

fn format_literal(v: &AlgebraicValue) -> String {
    match v {
        AlgebraicValue::String(s) => format!("'{}'", s.replace("'", "''")),
        _ => v.to_satn(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User;

    #[derive(Clone)]
    struct UserCols {
        pub id: Col<User, i32>,
        pub name: Col<User, String>,
        pub age: Col<User, i32>,
    }

    impl Default for UserCols {
        fn default() -> Self {
            Self {
                id: Col::new("id"),
                name: Col::new("name"),
                age: Col::new("age"),
            }
        }
    }

    impl TableName for User {
        const TABLE_NAME: &'static str = "users";
    }

    impl HasCols for User {
        type Cols = UserCols;
        fn cols() -> Self::Cols {
            UserCols::default()
        }
    }

    fn users() -> Table<User> {
        Table::new()
    }

    fn other() -> Table<Other> {
        Table::new()
    }

    struct IxUserCols {
        pub id: IxCol<User, i32>,
    }

    impl HasIxCols for User {
        type IxCols = IxUserCols;
        fn idx_cols() -> Self::IxCols {
            IxUserCols { id: IxCol::new("id") }
        }
    }

    // --- define "Other" table + IxCols ---
    struct Other;
    impl TableName for Other {
        const TABLE_NAME: &'static str = "other";
    }

    #[derive(Clone)]
    struct IxOtherCols {
        pub uid: IxCol<Other, i32>,
    }

    impl HasIxCols for Other {
        type IxCols = IxOtherCols;
        fn idx_cols() -> Self::IxCols {
            IxOtherCols { uid: IxCol::new("uid") }
        }
    }

    fn norm(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn test_simple_select() {
        let q = users().build();
        assert_eq!(q.sql(), r#"SELECT * FROM "users""#);
    }

    #[test]
    fn test_where_literal() {
        let q = users().r#where(|c| c.id.eq(10)).build();
        let expected = r#"SELECT * FROM "users" WHERE ("users"."id" = 10)"#;
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_where_multiple_predicates() {
        let q = users().r#where(|c| c.id.eq(10)).r#where(|c| c.age.gt(18)).build();

        let expected = r#"SELECT * FROM "users" WHERE (("users"."id" = 10) AND ("users"."age" > 18))"#;
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_column_column_comparison() {
        let q = users().r#where(|c| c.age.gt(c.id)).build();

        let expected = r#"SELECT * FROM "users" WHERE ("users"."age" > "users"."id")"#;
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_ne_comparison() {
        let q = users().r#where(|c| c.name.neq("Shub".to_string())).build();
        assert!(q.sql().contains("name"), "Expected a name comparison");
        assert!(q.sql().contains("<>"));
    }

    #[test]
    fn test_format_expr_column_literal() {
        let expr = Expr::Eq(
            ValueExpr::Column(ColumnRef::<User>::new("id")),
            ValueExpr::Literal(AlgebraicValue::from(42)),
        );

        let sql = format_expr(&expr);
        assert!(sql.contains("id"), "Missing col");
        assert!(sql.contains("42"), "Missing literal");
    }

    #[test]
    fn test_format_semi_join_expr() {
        let user = users();
        let other = other();

        let sql = user.left_semijoin(other, |u, o| u.id.eq(o.uid)).build().sql;

        let expected = r#"SELECT "right".* FROM "users" "left" JOIN "other" "right" ON "left"."id" = "right"."uid""#;

        assert_eq!(sql, expected);
    }

    #[test]
    fn test_left_semijoin_with_where_expr() {
        let user = users();
        let o = other();
        let sql = user
            .left_semijoin(o, |u, o| u.id.eq(o.uid))
            .r#where(|u| u.id.eq(1))
            .r#where(|u| u.id.gt(10))
            .build()
            .sql;

        let expected = r#"SELECT "left".* FROM "users" "left" JOIN "other" "right" ON "left"."id" = "right"."uid" WHERE (("users"."id" = 1) AND ("users"."id" > 10))"#;

        assert_eq!(sql, expected);

        // Test chaining where before join
        let user = users();
        let other = other();
        let sql2 = user
            .r#where(|u| u.id.eq(1))
            .r#where(|u| u.id.gt(10))
            .left_semijoin(other, |u, o| u.id.eq(o.uid))
            .build()
            .sql;
        assert_eq!(sql2, expected);
    }

    #[test]
    fn test_right_semijoin_with_where_expr() {
        let user = users();
        let o = other();
        let sql = user
            .right_semijoin(o, |u, o| u.id.eq(o.uid))
            .r#where(|u| u.id.eq(1))
            .r#where(|u| u.id.gt(10))
            .build()
            .sql;

        let expected = r#"SELECT "right".* FROM "users" "left" JOIN "other" "right" ON "left"."id" = "right"."uid" WHERE (("users"."id" = 1) AND ("users"."id" > 10))"#;
        assert_eq!(sql, expected);

        // Change order
        let user = users();
        let o = other();
        let sql = user
            .r#where(|u| u.id.eq(1))
            .right_semijoin(o, |u, o| u.id.eq(o.uid))
            .r#where(|u| u.id.gt(10))
            .build()
            .sql;

        assert_eq!(sql, expected);
    }
}
