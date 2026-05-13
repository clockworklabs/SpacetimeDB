use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(accessor = admin)]
pub struct Admin {
    #[primary_key]
    pub identity: Identity,
}

#[table(accessor = banned)]
pub struct Banned {
    #[primary_key]
    pub identity: Identity,
}

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub name: String,
}

#[reducer]
pub fn add_admin(ctx: &ReducerContext, target: Identity) {
    if ctx.db.admin().identity().find(ctx.sender()).is_none() {
        panic!("not admin");
    }
    let _ = ctx.db.admin().try_insert(Admin { identity: target });
}

#[reducer]
pub fn ban_player(ctx: &ReducerContext, target: Identity) {
    if ctx.db.admin().identity().find(ctx.sender()).is_none() {
        panic!("not admin");
    }
    ctx.db.banned().insert(Banned { identity: target });
    if ctx.db.player().identity().find(target).is_some() {
        ctx.db.player().identity().delete(target);
    }
}

#[reducer]
pub fn join_game(ctx: &ReducerContext, name: String) {
    if ctx.db.banned().identity().find(ctx.sender()).is_some() {
        panic!("banned");
    }
    if ctx.db.player().identity().find(ctx.sender()).is_some() {
        panic!("already in game");
    }
    ctx.db.player().insert(Player {
        identity: ctx.sender(),
        name,
    });
}
