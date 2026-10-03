//! Compiles the client expansions of module syntax (proposal 0040) for declarations that the SDK test clients
//! do not contain: several tables on one row type, a column default, a type in a namespace,
//! a view with a primary key whose row type is not a table's,
//! the forms that codegen writes for per-table column attributes, and the client-only ones for explicit column names
//! and omitted tables,
//! and module source with raw identifiers and private items.
// The SDK's `browser` feature removes `run_threaded` and the other blocking methods,
// while the expansion gates them on `target_arch`, as generated bindings always have.
#![cfg(not(feature = "browser"))]
#![allow(dead_code)]

mod module {
    use spacetimedb_sdk as spacetimedb;

    #[derive(spacetimedb::SpacetimeType, Clone, Copy, PartialEq, Eq, Hash, Debug)]
    #[sats(name = "Namespace.Color")]
    pub enum NamespaceColor {
        Red,
        Green,
    }

    #[spacetimedb::table(accessor = logged_out_player, public)]
    #[spacetimedb::table(accessor = player, public)]
    pub struct Player {
        #[primary_key]
        pub identity: spacetimedb::Identity,
        #[unique]
        #[auto_inc]
        pub player_id: u64,
        #[default(Some(NamespaceColor::Red))]
        pub color: Option<NamespaceColor>,
    }

    #[derive(spacetimedb::SpacetimeType, Clone, PartialEq, Debug)]
    pub struct Score {
        pub player_id: u64,
        pub points: u32,
    }

    #[spacetimedb::view(accessor = scores, public, primary_key = player_id)]
    pub fn scores(ctx: &spacetimedb::ViewContext) -> Vec<Score>;

    #[spacetimedb::reducer(client_connected)]
    pub fn on_connect(ctx: &spacetimedb::ReducerContext);

    #[spacetimedb::procedure]
    pub fn total(ctx: &mut spacetimedb::ProcedureContext, player_id: u64) -> u32;

    spacetimedb::client_module! {
        types: [NamespaceColor, Score],
        tables: [logged_out_player, player],
        views: [scores],
        reducers: [on_connect],
        procedures: [total],
    }
}

use module::*;
use spacetimedb_sdk::{Identity, Table, TableWithPrimaryKey};

fn api(ctx: &DbConnection) {
    let _: Option<Player> = ctx.db.player().identity().find(&Identity::ZERO);
    let _: Option<Player> = ctx.db.logged_out_player().player_id().find(&0);
    ctx.db
        .logged_out_player()
        .on_update(|_ctx: &EventContext, _old, _new| {});
    let _: usize = ctx.db.scores().iter().count();
    let _: Option<Score> = ctx.db.scores().player_id().find(&0);
    ctx.db.scores().on_update(|_ctx, _old: &Score, _new: &Score| {});
    ctx.reducers.on_connect().unwrap();
    ctx.procedures
        .total_then(0, |_ctx: &ProcedureEventContext, res: Result<u32, _>| drop(res));
    let _ = Reducer::OnConnect;
}

/// Declarations as codegen writes them for what module syntax cannot express yet.
mod generated {
    use spacetimedb_sdk as spacetimedb;

    // Tables that share a row type but not its column constraints, as a C# or C++ module can declare them.
    #[spacetimedb::table(accessor = multi_table_1, public)]
    #[spacetimedb::table(accessor = multi_table_2, public)]
    pub struct MultiTableRow {
        #[index(btree, table = multi_table_1)]
        pub name: String,
        #[primary_key(table = multi_table_1)]
        #[auto_inc(table = multi_table_1)]
        pub foo: u32,
        #[unique(table = [multi_table_2])]
        pub bar: u32,
    }

    // Canonical column names under `CaseConversionPolicy::None`, and a raw identifier.
    #[spacetimedb::table(accessor = person, public)]
    pub struct Person {
        #[name("personId")]
        #[primary_key]
        pub person_id: u32,
        #[name("playerRef")]
        #[index(btree)]
        pub player_ref: u32,
        #[unique]
        pub r#type: u32,
    }

    // A private table, which codegen omits but whose row type a public view returns.
    #[spacetimedb::table(accessor = secret_player, omitted)]
    pub struct SecretPlayer {
        #[primary_key]
        pub id: u64,
        #[index(btree)]
        pub level: u32,
    }

    #[spacetimedb::view(accessor = visible_players, public, primary_key = id)]
    pub fn visible_players(ctx: &spacetimedb::ViewContext) -> impl spacetimedb::Query<SecretPlayer>;

    #[spacetimedb::reducer]
    pub fn set_type(ctx: &spacetimedb::ReducerContext, r#type: u32);

    spacetimedb::client_module! {
        types: [SecretPlayer],
        tables: [multi_table_1, multi_table_2, person],
        views: [visible_players],
        reducers: [set_type(r#type)],
    }

    fn api(ctx: &DbConnection) {
        use spacetimedb_sdk::TableWithPrimaryKey;
        let _: Option<MultiTableRow> = ctx.db.multi_table_1().foo().find(&0);
        ctx.db.multi_table_1().on_update(|_ctx, _old, _new| {});
        let _: Option<MultiTableRow> = ctx.db.multi_table_2().bar().find(&0);
        let _: Option<Person> = ctx.db.person().r#type().find(&0);
        ctx.reducers.set_type(0).unwrap();
        let _ = Reducer::SetType { r#type: 0 };
        let _: Option<SecretPlayer> = ctx.db.visible_players().id().find(&0);
    }

    #[test]
    fn query_builder_uses_canonical_column_names() {
        use spacetimedb_sdk::__codegen::__query_builder::Table;
        let person = || Table::<Person>::new("person");
        let sql = person().r#where(|c| c.player_ref.eq(1)).build().sql().to_string();
        assert!(sql.contains(r#""person"."playerRef""#), "{sql}");
        let sql = person()
            .left_semijoin(person(), |a, b| a.player_ref.eq(b.person_id))
            .build()
            .sql()
            .to_string();
        assert!(sql.contains(r#""person"."playerRef" = "person"."personId""#), "{sql}");
    }

    #[test]
    fn omitted_table_rows_have_indexed_columns() {
        use spacetimedb_sdk::__codegen::__query_builder::Table;
        let visible_players = || Table::<SecretPlayer>::new("visible_players");
        let sql = visible_players()
            .left_semijoin(visible_players(), |a, b| a.level.eq(b.level))
            .build()
            .sql()
            .to_string();
        assert!(
            sql.contains(r#""visible_players"."level" = "visible_players"."level""#),
            "{sql}"
        );
    }
}

/// Module source with private items. `client_module!` goes where it can name them.
mod private_items {
    use spacetimedb_sdk as spacetimedb;

    #[derive(spacetimedb::SpacetimeType, Clone, PartialEq, Debug)]
    struct Inner {
        x: u32,
    }

    // A public row with private fields, one of them of a private type.
    #[spacetimedb::table(accessor = holder, public)]
    pub struct Holder {
        #[primary_key]
        id: u32,
        inner: Inner,
    }

    #[spacetimedb::table(accessor = player, public)]
    struct Player {
        #[primary_key]
        id: u32,
    }

    #[spacetimedb::reducer]
    fn add_player(ctx: &spacetimedb::ReducerContext, id: u32);

    mod bindings {
        use super::*;

        spacetimedb::client_module! {
            types: [Inner],
            tables: [super::holder, super::player],
            reducers: [super::add_player(id)],
        }

        fn api(ctx: &DbConnection) {
            let _: Option<Holder> = ctx.db.holder().id().find(&0);
            let _: Option<Player> = ctx.db.player().id().find(&0);
            ctx.reducers.add_player(0).unwrap();
        }
    }
}

/// The client's `#[table]` makes the fields of a row `pub`, as they are in generated bindings,
/// so client code outside the module reads a field that module source keeps private.
fn read_private_field(holder: &private_items::Holder) -> u32 {
    holder.id
}
