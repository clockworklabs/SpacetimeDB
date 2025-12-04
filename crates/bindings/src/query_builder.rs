pub mod expr;
pub mod join;
pub mod table;

pub use expr::*;
pub use join::*;
pub use table::*;

pub struct Query<T> {
    pub(crate) sql: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Query<T> {
    pub fn new(sql: String) -> Self {
        Self {
            sql,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::{sats::i256, TimeDuration};

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
        Table::default()
    }
    fn other() -> Table<Other> {
        Table::default()
    }
    struct OtherCols {
        pub uid: Col<Other, i32>,
    }

    impl HasCols for Other {
        type Cols = OtherCols;
        fn cols() -> Self::Cols {
            OtherCols { uid: Col::new("uid") }
        }
    }
    struct IxUserCols {
        pub id: IxCol<User, i32>,
    }
    impl HasIxCols for User {
        type IxCols = IxUserCols;
        fn ix_cols() -> Self::IxCols {
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
        fn ix_cols() -> Self::IxCols {
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
        let q = users().r#where(|c| c.name.ne("Shub".to_string())).build();
        assert!(q.sql().contains("name"), "Expected a name comparison");
        assert!(q.sql().contains("<>"));
    }

    #[test]
    fn test_or_comparison() {
        let q = users()
            .r#where(|c| c.name.ne("Shub".to_string()).or(c.name.ne("Pop".to_string())))
            .build();

        let expected = r#"SELECT * FROM "users" WHERE (("users"."name" <> 'Shub') OR ("users"."name" <> 'Pop'))"#;
        assert_eq!(q.sql, expected);
    }

    #[test]
    fn test_format_expr_column_literal() {
        let expr = BoolExpr::Eq(
            Operand::Column(ColumnRef::<User>::new("id")),
            Operand::Literal(LiteralValue::new("42".to_string())),
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
            .r#where(|u| u.id.eq(1i32))
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
            .r#where(|o| o.uid.eq(1))
            .r#where(|o| o.uid.gt(10))
            .build()
            .sql;
        let expected = r#"SELECT "right".* FROM "users" "left" JOIN "other" "right" ON "left"."id" = "right"."uid" WHERE (("other"."uid" = 1) AND ("other"."uid" > 10))"#;
        assert_eq!(sql, expected);
    }

    #[test]
    fn test_right_semijoin_with_left_and_right_where_expr() {
        let user = users();
        let o = other();
        let sql = user
            .r#where(|u| u.id.eq(1))
            .right_semijoin(o, |u, o| u.id.eq(o.uid))
            .r#where(|o| o.uid.gt(10))
            .build()
            .sql;
        let expected = r#"SELECT "right".* FROM "users" "left" JOIN "other" "right" ON "left"."id" = "right"."uid" WHERE ("users"."id" = 1) AND ("other"."uid" > 10)"#;
        assert_eq!(sql, expected);
    }

    #[test]
    fn test_literals() {
        use spacetimedb_lib::{ConnectionId, Identity};

        struct Player;
        struct PlayerCols {
            score: Col<Player, i32>,
            name: Col<Player, String>,
            active: Col<Player, bool>,
            connection_id: Col<Player, ConnectionId>,
            cells: Col<Player, i256>,
            identity: Col<Player, Identity>,
            ts: Col<Player, spacetimedb_lib::Timestamp>,
            bytes: Col<Player, Vec<u8>>,
        }

        impl TableName for Player {
            const TABLE_NAME: &'static str = "player";
        }

        impl HasCols for Player {
            type Cols = PlayerCols;
            fn cols() -> Self::Cols {
                PlayerCols {
                    score: Col::new("score"),
                    name: Col::new("name"),
                    active: Col::new("active"),
                    connection_id: Col::new("connection_id"),
                    cells: Col::new("cells"),
                    identity: Col::new("identity"),
                    ts: Col::new("ts"),
                    bytes: Col::new("bytes"),
                }
            }
        }

        let q = Table::<Player>::default().r#where(|c| c.score.eq(100)).build();

        assert_eq!(q.sql, r#"SELECT * FROM "player" WHERE ("player"."score" = 100)"#);

        let q = Table::<Player>::default()
            .r#where(|c| c.name.ne("Alice".to_string()))
            .build();

        assert_eq!(q.sql, r#"SELECT * FROM "player" WHERE ("player"."name" <> 'Alice')"#);

        let q = Table::<Player>::default().r#where(|c| c.active.eq(true)).build();

        assert_eq!(q.sql, r#"SELECT * FROM "player" WHERE ("player"."active" = TRUE)"#);

        let q = Table::<Player>::default()
            .r#where(|c| c.connection_id.eq(ConnectionId::ZERO))
            .build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."connection_id" = 0x00000000000000000000000000000000)"#
        );

        let big_int: i256 = (i256::ONE << 120) * i256::from(-1);
        let q = Table::<Player>::default().r#where(|c| c.cells.gt(big_int)).build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."cells" > -1329227995784915872903807060280344576)"#,
        );

        let q = Table::<Player>::default()
            .r#where(|c| c.identity.ne(Identity::ONE))
            .build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."identity" <> 0x0000000000000000000000000000000000000000000000000000000000000001)"#
        );

        let ts = spacetimedb_lib::Timestamp::UNIX_EPOCH + TimeDuration::from_micros(1000);
        let q = Table::<Player>::default().r#where(|c| c.ts.eq(ts)).build();
        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."ts" = '1970-01-01T00:00:00.001+00:00')"#
        );

        let q = Table::<Player>::default()
            .r#where(|c| c.bytes.eq(vec![1, 2, 3, 4, 255]))
            .build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."bytes" = 0x01020304ff)"#
        );
    }
}
