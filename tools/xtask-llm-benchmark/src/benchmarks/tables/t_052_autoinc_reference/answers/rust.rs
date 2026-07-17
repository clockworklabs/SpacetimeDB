use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = parent, public)]
pub struct Parent {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
}

#[table(
    accessor = child,
    public,
    index(accessor = by_parent, btree(columns = [parent_id]))
)]
pub struct Child {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub parent_id: u64,
    pub name: String,
}

#[reducer]
pub fn create_family(ctx: &ReducerContext, parent_name: String, child_names: Vec<String>) {
    let parent = ctx.db.parent().insert(Parent {
        id: 0,
        name: parent_name,
    });
    for name in child_names {
        ctx.db.child().insert(Child {
            id: 0,
            parent_id: parent.id,
            name,
        });
    }
}
