use spacetimedb::{Query, ReducerContext, Table, ViewContext};

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
fn online_users(ctx: &ViewContext) -> Query<User> {
    ctx.from.user().r#where(|c| c.online.eq(true)).build()
}

#[spacetimedb::view(accessor = online_users_age, public)]
fn online_users_age(ctx: &ViewContext) -> Query<Person> {
    ctx.from
        .user()
        .r#where(|u| u.online.eq(true))
        .right_semijoin(ctx.from.person(), |u, p| u.identity.eq(p.identity))
        .build()
}

#[spacetimedb::view(accessor = offline_user_20_years_old, public)]
fn offline_user_in_twienties(ctx: &ViewContext) -> Query<User> {
    ctx.from
        .person()
        .filter(|p| p.age.eq(20))
        .right_semijoin(ctx.from.user(), |p, u| p.identity.eq(u.identity))
        .filter(|u| u.online.eq(false))
        .build()
}

#[spacetimedb::view(accessor = users_whos_age_is_known, public)]
fn users_whos_age_is_known(ctx: &ViewContext) -> Query<User> {
    ctx.from
        .user()
        .left_semijoin(ctx.from.person(), |p, u| p.identity.eq(u.identity))
        .build()
}

#[spacetimedb::view(accessor = users_who_are_above_20_and_below_30, public)]
fn users_who_are_above_20_and_below_30(ctx: &ViewContext) -> Query<Person> {
    ctx.from.person().r#where(|p| p.age.gt(20).and(p.age.lt(30))).build()
}

#[spacetimedb::view(accessor = users_who_are_above_eq_20_and_below_eq_30, public)]
fn users_who_are_above_eq_20_and_below_eq_30(ctx: &ViewContext) -> Query<Person> {
    ctx.from.person().r#where(|p| p.age.gte(20).and(p.age.lte(30))).build()
}
