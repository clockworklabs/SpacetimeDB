use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(accessor = person, index(accessor = name_idx, btree(columns = [name])))]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
