use spacetimedb::{AnonymousViewContext, Identity, Query, ReducerContext, Table, ViewContext};

#[spacetimedb::table(accessor = user, public)]
pub struct User {
    #[primary_key]
    identity: u8,
    name: String,
    online: bool,
}

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    #[primary_key]
    identity: u8,
    name: String,
    #[index(btree)]
    age: u8,
}

#[spacetimedb::table(accessor = pk_join_lhs, public)]
pub struct LeftPkJoinSource {
    #[primary_key]
    id: u8,
    ok: bool,
    #[index(btree)]
    identity: Identity,
}

#[spacetimedb::table(accessor = pk_join_rhs, public)]
pub struct RightPkJoinSource {
    #[primary_key]
    id: u8,
    ok: bool,
    #[index(btree)]
    identity: Identity,
}

#[spacetimedb::reducer(init)]
fn init(ctx: &ReducerContext) {
    ctx.db.user().insert(User {
        identity: 1,
        name: "Alice".to_string(),
        online: true,
    });

    ctx.db.user().insert(User {
        identity: 2,
        name: "BOB".to_string(),
        online: false,
    });

    ctx.db.user().insert(User {
        identity: 3,
        name: "POP".to_string(),
        online: false,
    });

    ctx.db.person().insert(Person {
        identity: 1,
        name: "Alice".to_string(),
        age: 30,
    });

    ctx.db.person().insert(Person {
        identity: 2,
        name: "BOB".to_string(),
        age: 20,
    });
}

#[spacetimedb::view(accessor = online_users, public)]
fn online_users(ctx: &ViewContext) -> impl Query<User> {
    ctx.from.user().r#where(|c| c.online)
}

#[spacetimedb::view(accessor = online_users_age, public)]
fn online_users_age(ctx: &ViewContext) -> impl Query<Person> {
    ctx.from
        .user()
        .r#where(|u| u.online)
        .right_semijoin(ctx.from.person(), |u, p| u.identity.eq(p.identity))
}

#[spacetimedb::view(accessor = offline_user_20_years_old, public)]
fn offline_user_in_twienties(ctx: &ViewContext) -> impl Query<User> {
    ctx.from
        .person()
        .filter(|p| p.age.eq(20))
        .right_semijoin(ctx.from.user(), |p, u| p.identity.eq(u.identity))
        .filter(|u| u.online.eq(false))
}

#[spacetimedb::view(accessor = users_whos_age_is_known, public)]
fn users_whos_age_is_known(ctx: &ViewContext) -> impl Query<User> {
    ctx.from
        .user()
        .left_semijoin(ctx.from.person(), |p, u| p.identity.eq(u.identity))
}

#[spacetimedb::view(accessor = users_who_are_above_20_and_below_30, public)]
fn users_who_are_above_20_and_below_30(ctx: &ViewContext) -> impl Query<Person> {
    ctx.from.person().r#where(|p| p.age.gt(20).and(p.age.lt(30)))
}

#[spacetimedb::view(accessor = users_who_are_above_eq_20_and_below_eq_30, public)]
fn users_who_are_above_eq_20_and_below_eq_30(ctx: &ViewContext) -> impl Query<Person> {
    ctx.from.person().r#where(|p| p.age.gte(20).and(p.age.lte(30)))
}

#[spacetimedb::reducer]
fn update_pk_join_lhs(ctx: &ReducerContext, id: u8, ok: bool) {
    ctx.db.pk_join_lhs().id().delete(&id);
    ctx.db.pk_join_lhs().insert(LeftPkJoinSource {
        id,
        ok,
        identity: ctx.sender(),
    });
}

#[spacetimedb::reducer]
fn delete_pk_join_lhs(ctx: &ReducerContext, id: u8) {
    ctx.db.pk_join_lhs().id().delete(&id);
}

#[spacetimedb::reducer]
fn update_pk_join_rhs(ctx: &ReducerContext, id: u8, ok: bool) {
    ctx.db.pk_join_rhs().id().delete(&id);
    ctx.db.pk_join_rhs().insert(RightPkJoinSource {
        id,
        ok,
        identity: ctx.sender(),
    });
}

#[spacetimedb::reducer]
fn delete_pk_join_rhs(ctx: &ReducerContext, id: u8) {
    ctx.db.pk_join_rhs().id().delete(&id);
}

#[spacetimedb::view(accessor = pk_join_lhs_view, public)]
fn pk_join_lhs_view(ctx: &AnonymousViewContext) -> impl Query<LeftPkJoinSource> {
    ctx.from.pk_join_lhs()
}

#[spacetimedb::view(accessor = pk_join_rhs_view, public)]
fn pk_join_rhs_view(ctx: &AnonymousViewContext) -> impl Query<RightPkJoinSource> {
    ctx.from.pk_join_rhs()
}

#[spacetimedb::view(accessor = pk_join_lhs_sender_view, public)]
fn pk_join_lhs_sender_view(ctx: &ViewContext) -> impl Query<LeftPkJoinSource> {
    ctx.from.pk_join_lhs().filter(|row| row.identity.eq(ctx.sender()))
}

#[spacetimedb::view(accessor = pk_join_rhs_sender_view, public)]
fn pk_join_rhs_sender_view(ctx: &ViewContext) -> impl Query<RightPkJoinSource> {
    ctx.from.pk_join_rhs().filter(|row| row.identity.eq(ctx.sender()))
}
