use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = user)]
pub struct User {
    #[primary_key]
    #[auto_inc]
    pub user_id: u64,
    pub name: String,
}

#[table(accessor = group)]
pub struct Group {
    #[primary_key]
    #[auto_inc]
    pub group_id: u64,
    pub title: String,
}

#[table(
    accessor = membership,
    index(accessor = by_user,  btree(columns = [user_id])),
    index(accessor = by_group, btree(columns = [group_id]))
)]
pub struct Membership {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub user_id: u64,
    pub group_id: u64,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.user().insert(User {
        user_id: 0,
        name: "Alice".into(),
    });
    ctx.db.user().insert(User {
        user_id: 0,
        name: "Bob".into(),
    });

    ctx.db.group().insert(Group {
        group_id: 0,
        title: "Admin".into(),
    });
    ctx.db.group().insert(Group {
        group_id: 0,
        title: "Dev".into(),
    });

    ctx.db.membership().insert(Membership {
        id: 0,
        user_id: 1,
        group_id: 1,
    });
    ctx.db.membership().insert(Membership {
        id: 0,
        user_id: 1,
        group_id: 2,
    });
    ctx.db.membership().insert(Membership {
        id: 0,
        user_id: 2,
        group_id: 2,
    });
}
