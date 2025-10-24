use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = users)]
pub struct User {
    #[primary_key]
    pub user_id: i32,
    pub name: String,
}

#[table(name = groups)]
pub struct Group {
    #[primary_key]
    pub group_id: i32,
    pub title: String,
}

#[table(
    name = memberships,
    index(name = by_user,  btree(columns = [user_id])),
    index(name = by_group, btree(columns = [group_id]))
)]
pub struct Membership {
    #[primary_key]
    pub id: i32,
    pub user_id: i32,
    pub group_id: i32,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.users().insert(User  { user_id: 1, name: "Alice".into() });
    ctx.db.users().insert(User  { user_id: 2, name: "Bob".into()   });

    ctx.db.groups().insert(Group { group_id: 10, title: "Admin".into() });
    ctx.db.groups().insert(Group { group_id: 20, title: "Dev".into()   });

    ctx.db.memberships().insert(Membership { id: 1, user_id: 1, group_id: 10 });
    ctx.db.memberships().insert(Membership { id: 2, user_id: 1, group_id: 20 });
    ctx.db.memberships().insert(Membership { id: 3, user_id: 2, group_id: 20 });
}
