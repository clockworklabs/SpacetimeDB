use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(accessor = user)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    pub role: String,
}

#[reducer]
pub fn register(ctx: &ReducerContext) {
    if ctx.db.user().identity().find(ctx.sender()).is_some() {
        panic!("already registered");
    }
    ctx.db.user().insert(User {
        identity: ctx.sender(),
        role: "member".to_string(),
    });
}

#[reducer]
pub fn promote(ctx: &ReducerContext, target: Identity) {
    let caller = ctx.db.user().identity().find(ctx.sender()).expect("not registered");
    if caller.role != "admin" {
        panic!("not admin");
    }
    let mut target_user = ctx.db.user().identity().find(target).expect("target not registered");
    target_user.role = "admin".to_string();
    ctx.db.user().identity().update(target_user);
}

#[reducer]
pub fn member_action(ctx: &ReducerContext) {
    ctx.db.user().identity().find(ctx.sender()).expect("not registered");
}

#[reducer]
pub fn admin_action(ctx: &ReducerContext) {
    let user = ctx.db.user().identity().find(ctx.sender()).expect("not registered");
    if user.role != "admin" {
        panic!("not admin");
    }
}
