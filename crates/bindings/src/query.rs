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
    _marker: std::marker::PhantomData<T>,
}

impl<T> Table<T> {
    pub fn new() -> Self {
        Table {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: HasCols> Table<T> {
    pub fn build(self) -> Query {
        Query {
            sql: format!("SELECT * FROM {}", T::TABLE_NAME),
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

impl<L: HasIxCols> Table<L> {
    pub fn left_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> JoinWhere<L, R>
    where
        R: HasIxCols,
    {
        let lix = L::idx_cols();
        let rix = R::idx_cols();
        let join = on(&lix, &rix);
        JoinWhere {
            left_col: join.lhs_col,
            right_col: join.rhs_col,
        }
    }
}

pub struct Col<T, V> {
    pub(crate) column_name: &'static str,
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

/// NOTE: no `V: Serialize` bound here.
/// That’s the key: column–column comparisons work even if V is not serializable.
/// The constraint only applies on the RHS when you actually use a literal V.
impl<T: TableName, V> Col<T, V> {
    pub fn eq<R>(self, rhs: R) -> Expr<ValueExpr<T>>
    where
        R: RHS<T, V>,
    {
        Expr::Eq(self.into(), rhs.to_expr())
    }

    pub fn neq<R>(self, rhs: R) -> Expr<ValueExpr<T>>
    where
        R: RHS<T, V>,
    {
        Expr::Neq(self.into(), rhs.to_expr())
    }

    pub fn gt<R>(self, rhs: R) -> Expr<ValueExpr<T>>
    where
        R: RHS<T, V>,
    {
        Expr::Gt(self.into(), rhs.to_expr())
    }

    pub fn lt<R>(self, rhs: R) -> Expr<ValueExpr<T>>
    where
        R: RHS<T, V>,
    {
        Expr::Lt(self.into(), rhs.to_expr())
    }
}

pub struct IxCol<T, V> {
    col: ColumnRef<T>,
    _marker: std::marker::PhantomData<V>,
}

impl<T, V> IxCol<T, V> {
    pub fn new(column_name: &'static str) -> Self {
        IxCol {
            col: ColumnRef {
                column_name,
                _marker: std::marker::PhantomData,
            },
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, V> Clone for IxCol<T, V> {
    fn clone(&self) -> Self {
        IxCol {
            col: ColumnRef {
                column_name: self.col.column_name,
                _marker: PhantomData,
            },
            _marker: PhantomData,
        }
    }
}

impl<T, V> Copy for IxCol<T, V> {}

impl<T, V> IxCol<T, V> {
    pub fn eq<R: HasIxCols>(self, rhs: IxCol<R, V>) -> IxJoinEq<T, R, V>
where {
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
        let cols = T::cols();
        let expr = f(&cols);

        FromWhere {
            expr: self.expr.and(expr),
        }
    }

    pub fn build(self) -> Query {
        let where_clause = format_expr(&self.expr);

        Query {
            sql: format!("SELECT * FROM {} WHERE {}", T::TABLE_NAME, where_clause),
        }
    }
}

impl<L: HasIxCols> FromWhere<L> {
    pub fn left_semijoin<R: HasIxCols, V>(
        self,
        _right: Table<R>,
        on: impl Fn(&L::IxCols, &R::IxCols) -> IxJoinEq<L, R, V>,
    ) -> JoinWhere<L, R>
    where
        R: HasIxCols,
    {
        let lix = L::idx_cols();
        let rix = R::idx_cols();
        let join = on(&lix, &rix);
        JoinWhere::new(join)
    }
}

/// After a join.
pub struct JoinWhere<T, R> {
    left_col: ColumnRef<T>,
    right_col: ColumnRef<R>,
    where_expr: Option<Expr<ValueExpr<T>>>,
}

impl<T, R> JoinWhere<T, R> {
    fn new<V>(join: IxJoinEq<T, R, V>) -> Self {
        JoinWhere {
            left_col: join.lhs_col,
            right_col: join.rhs_col,
            where_expr: None,
        }
    }
}

impl<T: HasCols, R: HasCols> JoinWhere<T, R> {
    pub fn r#where<F>(self, f: F) -> Self
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let cols = T::cols();
        let expr = f(&cols);

        JoinWhere {
            left_col: self.left_col,
            right_col: self.right_col,
            //TODO: combine with existing join condition
            where_expr: Some(expr),
        }
    }

    pub fn build(self) -> Query {
        Query {
            sql: String::from("SELECT * FROM ..."), // Placeholder
        }
    }
}
/// RHS of a comparison: either a column of the same table/column-type, or a literal V: Serialize.
/// The `<T, V>` here preserves type safety: you can only compare compatible things.
pub trait RHS<T, V> {
    fn to_expr(self) -> ValueExpr<T>;
}

/// RHS is a column of the same table and same Rust type V.
impl<T, V> RHS<T, V> for Col<T, V> {
    fn to_expr(self) -> ValueExpr<T> {
        ValueExpr::Column(ColumnRef {
            column_name: self.column_name,
            _marker: std::marker::PhantomData,
        })
    }
}

/// RHS is a literal of type V, which must be serializable to AlgebraicValue.
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
        ColumnRef {
            column_name: self.column_name,
            _marker: PhantomData,
        }
    }
}
impl<T> Copy for ColumnRef<T> {}

pub enum ValueExpr<T> {
    Column(ColumnRef<T>),
    Literal(AlgebraicValue),
}

impl<T: TableName, V> From<Col<T, V>> for ValueExpr<T> {
    fn from(col: Col<T, V>) -> Self {
        ValueExpr::Column(ColumnRef {
            column_name: col.column_name,
            _marker: PhantomData,
        })
    }
}

pub enum Expr<T> {
    Eq(T, T),
    Neq(T, T),
    Gt(T, T),
    Lt(T, T),
    And(Box<Expr<T>>, Box<Expr<T>>),

    // Semi-join predicate encoded as EXISTS subquery
    ExistsSemiJoin {
        right_table: &'static str,
        lhs_col: &'static str,
        rhs_col: &'static str,
    },
}

impl<T> Expr<T> {
    pub fn and(self, other: Expr<T>) -> Expr<T> {
        Expr::And(Box::new(self), Box::new(other))
    }
}

pub fn format_expr<T: TableName>(expr: &Expr<ValueExpr<T>>) -> String {
    match expr {
        Expr::Eq(lhs, rhs) => format!("{} = {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::Neq(lhs, rhs) => format!("{} <> {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::Gt(lhs, rhs) => format!("{} > {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::Lt(lhs, rhs) => format!("{} < {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::And(a, b) => format!("({}) AND ({})", format_expr(a), format_expr(b)),
        _ => {
            // For simplicity, we only implement the above cases for now.
            unimplemented!("Expression formatting not implemented for this variant")
        }
    }
}

fn format_value_expr<T: TableName>(v: &ValueExpr<T>) -> String {
    match v {
        ValueExpr::Column(ColumnRef { column_name, .. }) => format!("{}.{}", T::TABLE_NAME, column_name),
        ValueExpr::Literal(av) => format_algebraic_literal(av),
    }
}

fn format_algebraic_literal(v: &AlgebraicValue) -> String {
    v.to_satn()
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

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

    fn norm(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    // ---- Tests -------------------------------------------------------------

    #[test]
    fn test_simple_select() {
        let q = users().build();
        assert_eq!(q.sql(), "SELECT * FROM users");
    }

    #[test]
    fn test_where_literal() {
        let q = users().r#where(|c| c.id.eq(10)).build();

        let expected = "SELECT * FROM users WHERE id = 10";
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_where_multiple_predicates() {
        let q = users().r#where(|c| c.id.eq(10)).r#where(|c| c.age.gt(18)).build();

        let expected = "SELECT * FROM users WHERE (id = 10) AND (age > 18)";
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_column_column_comparison() {
        // age > id
        let q = users().r#where(|c| c.age.gt(c.id)).build();

        let expected = "SELECT * FROM users WHERE age > id";
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_ne_comparison() {
        let q = users().r#where(|c| c.name.neq("Shub".to_string())).build();

        // This uses Debug formatting for literal (your current implementation)
        let contains = q.sql().contains("name <> ");
        assert!(contains, "Query did not contain `name <> ...`");
    }

    #[test]
    fn test_format_expr_column_literal() {
        let expr = Expr::Eq(
            ValueExpr::Column(ColumnRef {
                column_name: "id",
                _marker: PhantomData::<User>,
            }),
            ValueExpr::Literal(AlgebraicValue::from(42)),
        );

        let sql = format_expr(&expr);
        assert!(sql.contains("id ="), "left side missing");
        assert!(sql.contains("42"), "right literal missing");
    }
}
