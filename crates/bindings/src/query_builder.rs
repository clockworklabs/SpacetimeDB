pub mod expr;
pub mod join;
pub mod table;

pub use expr::*;
pub use join::*;
pub use table::*;

pub struct Query {
    pub(crate) sql: String,
}

impl Query {
    pub fn sql(&self) -> &str {
        &self.sql
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
    #[test]
    fn test_or_comparison() {
        let q = users()
            .r#where(|c| c.name.neq("Shub".to_string()).or(c.name.neq("Pop".to_string())))
            .build();

        let expected = r#"SELECT * FROM "users" WHERE (("users"."name" <> 'Shub') OR ("users"."name" <> 'Pop'))"#;
        assert_eq!(q.sql, expected);
    }

    #[test]
    fn test_format_expr_column_literal() {
        let expr = Expr::Eq(
            ValueExpr::Column(ColumnRef::<User>::new("id")),
            ValueExpr::Literal(crate::AlgebraicValue::from(42)),
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
        let expected = r#"SELECT "left".* FROM "users" "left" JOIN "other" "right" ON "left"."id" = "right"."uid""#;
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
