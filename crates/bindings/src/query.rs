use spacetimedb_lib::{AlgebraicValue, sats::{algebraic_value::ser::ValueSerializer, satn::Satn}, ser::Serialize};

pub struct QueryBuilder {}

pub struct Table<T> {
    table_name: &'static str,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Table<T> {
    pub fn new(table_name: &'static str) -> Self {
        Table {
            table_name,
            _marker: std::marker::PhantomData,
        }
    }

    /// Simple SELECT * with no WHERE
    pub fn build(self) -> Query {
        Query {
            sql: format!("SELECT * FROM {}", self.table_name),
        }
    }
}

pub struct FromWhere<T> {
    table_name: &'static str,
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
            table_name: self.table_name,
            expr: self.expr.and(expr)
        }
    }

    pub fn build(self) -> Query {
        let where_clause = format_expr(&self.expr);

        Query {
            sql: format!("SELECT * FROM {} WHERE {}", self.table_name, where_clause),
        }
    }
}

pub struct JoinWhere<T> {
    _marker: std::marker::PhantomData<T>,
}

pub struct Query {
    sql: String,
}

impl Query {
    pub fn sql(&self) -> &str {
        &self.sql
    }
}

pub trait HasCols {
    type Cols;

    fn cols() -> Self::Cols;
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

impl<T: HasCols> Table<T> {
    pub fn r#where<F>(self, f: F) -> FromWhere<T>
    where
        F: Fn(&T::Cols) -> Expr<ValueExpr<T>>,
    {
        let cols = T::cols();
        let expr = f(&cols);
        FromWhere {
            table_name: self.table_name,
            expr,
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
        ValueExpr::Column {
            name: self.column_name,
            _marker: std::marker::PhantomData,
        }
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

/// NOTE: no `V: Serialize` bound here.
/// That’s the key: column–column comparisons work even if V is not serializable.
/// The constraint only applies on the RHS when you actually use a literal V.
impl<T, V> Col<T, V> {
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


pub fn format_expr<T>(expr: &Expr<ValueExpr<T>>) -> String {
    match expr {
        Expr::Eq(lhs, rhs) => format!("{} = {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::Neq(lhs, rhs) => format!("{} <> {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::Gt(lhs, rhs) => format!("{} > {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::Lt(lhs, rhs) => format!("{} < {}", format_value_expr(lhs), format_value_expr(rhs)),
        Expr::And(a, b) => format!("({}) AND ({})", format_expr(a), format_expr(b)),
    }
}

fn format_value_expr<T>(v: &ValueExpr<T>) -> String {
    match v {
        ValueExpr::Column { name, .. } => name.to_string(),
        ValueExpr::Literal(av) => format_algebraic_literal(av),
    }
}

fn format_algebraic_literal(v: &AlgebraicValue) -> String {
    v.to_satn()
}


#[cfg(test)]
mod tests {
    use std::{default, marker::{PhantomData, PhantomPinned}};

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

    impl HasCols for User {
        type Cols = UserCols;
        fn cols() -> Self::Cols {
            UserCols::default()
        }
    }

    fn users() -> Table<User> {
        Table::new("users")
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
        let q = users()
            .r#where(|c| c.id.eq(10))
            .build();

        let expected = "SELECT * FROM users WHERE id = 10";
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_where_multiple_predicates() {
        let q = users()
            .r#where(|c| c.id.eq(10))
            .r#where(|c| c.age.gt(18))
            .build();

        let expected =
            "SELECT * FROM users WHERE (id = 10) AND (age > 18)";
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_column_column_comparison() {
        // age > id
        let q = users()
            .r#where(|c| c.age.gt(c.id))
            .build();

        let expected = "SELECT * FROM users WHERE age > id";
        assert_eq!(norm(q.sql()), norm(expected));
    }

    #[test]
    fn test_ne_comparison() {
        let q = users()
            .r#where(|c| c.name.neq("Shub".to_string()))
            .build();

        // This uses Debug formatting for literal (your current implementation)
        let contains = q.sql().contains("name <> ");
        assert!(contains, "Query did not contain `name <> ...`");
    }

    #[test]
    fn test_format_expr_column_literal() {
        let expr = Expr::Eq(
            ValueExpr::Column { name: "id", _marker: PhantomData::<User> },
            ValueExpr::Literal(AlgebraicValue::from(42)),
        );

        let sql = format_expr(&expr);
        assert!(sql.contains("id ="), "left side missing");
        assert!(sql.contains("42"), "right literal missing");
    }
}
