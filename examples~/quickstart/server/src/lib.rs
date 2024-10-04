use spacetimedb::{ReducerContext, Identity, Table, Timestamp};
use anyhow::{Result, anyhow};

#[spacetimedb::table(name = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    online: bool,
}

#[spacetimedb::table(name = message, public)]
pub struct Message {
    sender: Identity,
    sent: Timestamp,
    text: String,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {
	
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(&ctx.sender) {
        // If this is a returning user, i.e. we already have a `User` with this `Identity`,
        // set `online: true`, but leave `name` and `identity` unchanged.
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        // If this is a new user, create a `User` row for the `Identity`,
        // which is online, but hasn't set a name.
        ctx.db.user().try_insert(User {
            name: None,
            identity: ctx.sender,
            online: true,
        }).unwrap();
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(&ctx.sender) {
        ctx.db.user().identity().update(User { online: false, ..user });
    } else {
        // This branch should be unreachable,
        // as it doesn't make sense for a client to disconnect without connecting first.
        log::warn!("Disconnect event for unknown user with identity {:?}", ctx.sender);
    }
}

fn validate_name(name: String) -> Result<String> {
    if name.is_empty() {
        Err(anyhow!("Names must not be empty"))
    } else {
        Ok(name)
    }
}

#[spacetimedb::reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<()> {
    let name = validate_name(name)?;
    if let Some(user) = ctx.db.user().identity().find(&ctx.sender) {
        ctx.db.user().identity().update(User { name: Some(name), ..user });
        Ok(())
    } else {
        Err(anyhow!("Cannot set name for unknown user"))
    }
}

fn validate_message(text: String) -> Result<String> {
    if text.is_empty() {
        Err(anyhow!("Messages must not be empty"))
    } else {
        Ok(text)
    }
}

#[spacetimedb::reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<()> {
    // Things to consider:
    // - Rate-limit messages per-user.
    // - Reject messages from unnamed users.
    let text = validate_message(text)?;
    ctx.db.message().insert(Message {
        sender: ctx.sender,
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}
