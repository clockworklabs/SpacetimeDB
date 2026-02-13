use spacetimedb::{table, view, Identity, Query, ViewContext};

#[table(name = player_info)]
struct PlayerInfo {
    #[unique]
    identity: Identity,
    #[index(btree)]
    weight: u32,
    age: u8,
}
/// Comparing incompatible types in `where` condition: u8 != u32 implicitly
#[view(name = view_bad_where_int_types_implicit, public)]
fn view_bad_where_int_types_implicit(ctx: &ViewContext) -> impl Query<PlayerInfo> {
    ctx.from.player_info().r#where(|a| a.age.eq(4200)).build()
}

fn main() {}
