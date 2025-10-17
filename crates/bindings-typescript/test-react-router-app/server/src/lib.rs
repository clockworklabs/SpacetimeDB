use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(public, name = counter)]
struct Counter {
    #[primary_key]
    id: u32,
    count: u32,
}

#[table(public, name = user)]
#[table(public, name = offline_user)]
struct User {
    #[primary_key]
    identity: Identity,
    has_incremented_count: u32,
}

#[reducer(init)]
fn init(ctx: &ReducerContext) {
    ctx.db.counter().insert(Counter { id: 0, count: 0 });
}

#[reducer(client_connected)]
fn client_connected(ctx: &ReducerContext) {
    let existing_user = ctx.db.offline_user().identity().find(ctx.sender);
    if let Some(user) = existing_user {
        ctx.db.user().insert(user);
        ctx.db.offline_user().identity().delete(ctx.sender);
        return;
    }
    ctx.db.offline_user().insert(User {
        identity: ctx.sender,
        has_incremented_count: 0,
    });
}

#[reducer(client_disconnected)]
fn client_disconnected(ctx: &ReducerContext) -> Result<(), String> {
    let existing_user = ctx.db.user().identity().find(ctx.sender).ok_or("User not found")?;
    ctx.db.offline_user().insert(existing_user);
    ctx.db.user().identity().delete(ctx.sender);
    Ok(())
}

#[reducer]
fn increment_counter(ctx: &ReducerContext) -> Result<(), String> {
    let mut counter = ctx.db.counter().id().find(0).ok_or("Counter not found")?;
    counter.count += 1;
    ctx.db.counter().id().update(counter);

    let mut user = ctx.db.user().identity().find(ctx.sender).ok_or("User not found")?;
    user.has_incremented_count += 1;
    ctx.db.user().identity().update(user);

    Ok(())
}

#[reducer]
fn clear_counter(ctx: &ReducerContext) {
    for row in ctx.db.counter().iter() {
        ctx.db.counter().id().delete(row.id);
    }

    for row in ctx.db.user().iter() {
        let user = User {
            identity: row.identity,
            has_incremented_count: 0,
        };
        ctx.db.user().identity().update(user);
    }

    for row in ctx.db.offline_user().iter() {
        let user = User {
            identity: row.identity,
            has_incremented_count: 0,
        };
        ctx.db.offline_user().identity().update(user);
    }
}
