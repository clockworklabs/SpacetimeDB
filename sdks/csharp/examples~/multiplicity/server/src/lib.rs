use spacetimedb::{table, reducer,ReducerContext, Table};

#[table(name = dog, public)]
pub struct Dog {
    #[primary_key]
    name: String,
    color: String,
    age: u32
}

#[table(name = cat, public)]
pub struct Cat {
    #[primary_key]
    name: String,
    color: String,
    age: u32
}

#[reducer]
pub fn add_dog(ctx: &ReducerContext, name: String, color: String, age: u32) {
    ctx.db.dog().insert(Dog { name, color, age });
}

#[reducer]
pub fn add_cat(ctx: &ReducerContext, name: String, color: String, age: u32) {
    ctx.db.cat().insert(Cat { name, color, age });
}

#[reducer]
pub fn update_dog(ctx: &ReducerContext, name: String, color: String, age: u32) -> Result<(), String> {
    if let Some(dog) = ctx.db.dog().name().find(&name) {
        ctx.db.dog().name().update(Dog {
            name: name,
            color: color,
            age: age,
            ..dog
        });
        Ok(())
    } else {
        Err("Cannot update unknown dog".to_string())
    }
}

#[reducer]
pub fn update_cat(ctx: &ReducerContext, name: String, color: String, age: u32) -> Result<(), String> {
    if let Some(cat) = ctx.db.cat().name().find(&name) {
        ctx.db.cat().name().update(Cat {
            name: name,
            color: color,
            age: age,
            ..cat
        });
        Ok(())
    } else {
        Err("Cannot update unknown cat".to_string())
    }
}

#[reducer]
pub fn remove_dog(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if let Some(dog) = ctx.db.dog().name().find(name.to_string()) {
        ctx.db.dog().name().delete(&dog.name);
        log::info!("Deleted dog named {:?}", name);
        Ok(())
    } else {
        Err("Cannot delete unknown dog".to_string())
    }
}

#[reducer]
pub fn remove_cat(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if let Some(cat) = ctx.db.cat().name().find(name.to_string()) {
        ctx.db.cat().name().delete(&cat.name);
        log::info!("Deleted cat named {:?}", name);
        Ok(())
    } else {
        Err("Cannot delete unknown cat".to_string())
    }
}

#[reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    // Called when the module is initially published
}

#[reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) {
    // Called everytime a new client connects
}

#[reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    // Called everytime a client disconnects
}