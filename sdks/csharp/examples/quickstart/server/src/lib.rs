use spacetimedb::{spacetimedb, ReducerContext, Identity, Timestamp};
use anyhow::{Result, anyhow};

#[spacetimedb(table)]
pub struct User {
    #[primarykey]
    identity: Identity,
    name: Option<String>,
    online: bool,
}

#[spacetimedb(table)]
pub struct Message {
    sender: Identity,
    sent: Timestamp,
    text: String,
}

#[spacetimedb(init)]
pub fn init() {
    // Called when the module is initially published
}

#[spacetimedb(connect)]
pub fn identity_connected(ctx: ReducerContext) {
    if let Some(user) = User::filter_by_identity(&ctx.sender) {
        // If this is a returning user, i.e. we already have a `User` with this `Identity`,
        // set `online: true`, but leave `name` and `identity` unchanged.
        User::update_by_identity(&ctx.sender, User { online: true, ..user });
    } else {
        // If this is a new user, create a `User` row for the `Identity`,
        // which is online, but hasn't set a name.
        User::insert(User {
            name: None,
            identity: ctx.sender,
            online: true,
        }).unwrap();
    }
}

#[spacetimedb(disconnect)]
pub fn identity_disconnected(ctx: ReducerContext) {
    if let Some(user) = User::filter_by_identity(&ctx.sender) {
        User::update_by_identity(&ctx.sender, User { online: false, ..user });
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

#[spacetimedb(reducer)]
pub fn set_name(ctx: ReducerContext, name: String) -> Result<()> {
    let name = validate_name(name)?;
    if let Some(user) = User::filter_by_identity(&ctx.sender) {
        User::update_by_identity(&ctx.sender, User { name: Some(name), ..user });
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

#[spacetimedb(reducer)]
pub fn send_message(ctx: ReducerContext, text: String) -> Result<()> {
    // Things to consider:
    // - Rate-limit messages per-user.
    // - Reject messages from unnamed users.
    let text = validate_message(text)?;
    Message::insert(Message {
        sender: ctx.sender,
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}
