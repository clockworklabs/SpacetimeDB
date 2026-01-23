pub mod expr;
pub mod join;
pub mod table;

pub use expr::*;
pub use join::*;
use spacetimedb_lib::{sats::impl_st, AlgebraicType, SpacetimeType};
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

impl_st!([T: SpacetimeType] Query<T>, ts => AlgebraicType::option(T::make_type(ts)));

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
    impl UserCols {
        fn new(table_name: &'static str) -> Self {
            Self {
                id: Col::new(table_name, "id"),
                name: Col::new(table_name, "name"),
                age: Col::new(table_name, "age"),
            }
        }
    }
    impl HasCols for User {
        type Cols = UserCols;
        fn cols(table_name: &'static str) -> Self::Cols {
            UserCols::new(table_name)
        }
    }
    fn users() -> Table<User> {
        Table::new("users")
    }
    fn other() -> Table<Other> {
        Table::new("other")
    }
    struct OtherCols {
        pub uid: Col<Other, i32>,
    }

    impl HasCols for Other {
        type Cols = OtherCols;
        fn cols(table: &'static str) -> Self::Cols {
            OtherCols {
                uid: Col::new(table, "uid"),
            }
        }
    }
    struct IxUserCols {
        pub id: IxCol<User, i32>,
    }
    impl HasIxCols for User {
        type IxCols = IxUserCols;
        fn ix_cols(table_name: &'static str) -> Self::IxCols {
            IxUserCols {
                id: IxCol::new(table_name, "id"),
            }
        }
    }
    struct Other;
    #[derive(Clone)]
    struct IxOtherCols {
        pub uid: IxCol<Other, i32>,
    }
    impl HasIxCols for Other {
        type IxCols = IxOtherCols;
        fn ix_cols(table_name: &'static str) -> Self::IxCols {
            IxOtherCols {
                uid: IxCol::new(table_name, "uid"),
            }
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
    fn test_where_gte_lte() {
        let q = users().r#where(|c| c.age.gte(18)).r#where(|c| c.age.lte(30)).build();
        let expected = r#"SELECT * FROM "users" WHERE (("users"."age" >= 18) AND ("users"."age" <= 30))"#;
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
    fn test_filter_alias() {
        let q = users().filter(|c| c.id.eq(5)).filter(|c| c.age.lt(30)).build();
        let expected = r#"SELECT * FROM "users" WHERE (("users"."id" = 5) AND ("users"."age" < 30))"#;
        assert_eq!(norm(q.sql()), norm(expected));
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
            Operand::Column(ColumnRef::<User>::new("user", "id")),
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
        let expected = r#"SELECT "users".* FROM "users" JOIN "other" ON "users"."id" = "other"."uid""#;
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
        let expected = r#"SELECT "users".* FROM "users" JOIN "other" ON "users"."id" = "other"."uid" WHERE (("users"."id" = 1) AND ("users"."id" > 10))"#;
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
        let expected = r#"SELECT "other".* FROM "users" JOIN "other" ON "users"."id" = "other"."uid" WHERE (("other"."uid" = 1) AND ("other"."uid" > 10))"#;
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
        let expected = r#"SELECT "other".* FROM "users" JOIN "other" ON "users"."id" = "other"."uid" WHERE ("users"."id" = 1) AND ("other"."uid" > 10)"#;
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

        impl HasCols for Player {
            type Cols = PlayerCols;
            fn cols(table_name: &'static str) -> Self::Cols {
                PlayerCols {
                    score: Col::new(table_name, "score"),
                    name: Col::new(table_name, "name"),
                    active: Col::new(table_name, "active"),
                    connection_id: Col::new(table_name, "connection_id"),
                    cells: Col::new(table_name, "cells"),
                    identity: Col::new(table_name, "identity"),
                    ts: Col::new(table_name, "ts"),
                    bytes: Col::new(table_name, "bytes"),
                }
            }
        }

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.score.eq(100)).build();

        assert_eq!(q.sql, r#"SELECT * FROM "player" WHERE ("player"."score" = 100)"#);

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.name.ne("Alice".to_string())).build();

        assert_eq!(q.sql, r#"SELECT * FROM "player" WHERE ("player"."name" <> 'Alice')"#);

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.active.eq(true)).build();

        assert_eq!(q.sql, r#"SELECT * FROM "player" WHERE ("player"."active" = TRUE)"#);

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.connection_id.eq(ConnectionId::ZERO)).build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."connection_id" = 0x00000000000000000000000000000000)"#
        );

        let big_int: i256 = (i256::ONE << 120) * i256::from(-1);

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.cells.gt(big_int)).build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."cells" > -1329227995784915872903807060280344576)"#,
        );

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.identity.ne(Identity::ONE)).build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."identity" <> 0x0000000000000000000000000000000000000000000000000000000000000001)"#
        );

        let ts = spacetimedb_lib::Timestamp::UNIX_EPOCH + TimeDuration::from_micros(1000);

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.ts.eq(ts)).build();
        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."ts" = '1970-01-01T00:00:00.001+00:00')"#
        );

        let table = Table::<Player>::new("player");
        let q = table.r#where(|c| c.bytes.eq(vec![1, 2, 3, 4, 255])).build();

        assert_eq!(
            q.sql,
            r#"SELECT * FROM "player" WHERE ("player"."bytes" = 0x01020304ff)"#
        );
    }
}
