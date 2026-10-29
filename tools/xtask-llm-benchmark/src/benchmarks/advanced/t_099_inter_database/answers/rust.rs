#[path = "receiver/lib.rs"]
mod receiver;
use spacetimedb::{Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = receipt, public)]
pub struct Receipt {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: String,
    sender: Identity,
}
#[spacetimedb::reducer]
pub fn receive(ctx: &ReducerContext, body: String, reject: bool) -> Result<(), String> {
    ctx.db.receipt().insert(Receipt {
        id: 0,
        body,
        sender: ctx.sender(),
    });
    if reject {
        return Err("receiver rejected".into());
    }
    Ok(())
}
#[spacetimedb::table(accessor = outgoing, public, outbox(receiver::receive))]
pub struct Outgoing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
    body: String,
    reject: bool,
}
#[spacetimedb::table(accessor = delivery_result, public)]
pub struct DeliveryResult {
    #[primary_key]
    id: u64,
    body: String,
    succeeded: bool,
}
#[spacetimedb::reducer]
pub fn send_message(
    ctx: &ReducerContext,
    target: receiver::Identity,
    body: String,
    reject: bool,
    abort: bool,
) -> Result<(), String> {
    ctx.db.outgoing().insert(Outgoing {
        id: 0,
        target,
        body,
        reject,
    });
    if abort {
        return Err("sender rejected".into());
    }
    Ok(())
}
#[spacetimedb::reducer(on_result(self::outgoing))]
pub fn on_delivery(ctx: &ReducerContext, row: Outgoing, result: Result<(), String>) {
    ctx.db.delivery_result().insert(DeliveryResult {
        id: row.id,
        body: row.body,
        succeeded: result.is_ok(),
    });
}
