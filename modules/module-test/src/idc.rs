use spacetimedb::ReducerContext;

// Reuse the generated receiver bindings exercised by the bindings UI tests.
#[path = "../../../crates/bindings/tests/fixtures/idc_receiver/lib.rs"]
mod receiver;

#[spacetimedb::table(
    accessor = idc_outbox,
    name = "idc_outbox",
    public,
    outbox(receiver::receive_ping),
    index(accessor = by_tag, btree(columns = [tag])),
)]
pub struct IdcOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    tag: String,
    #[target]
    target: receiver::Identity,
    #[param(name = body)]
    message: String,
}

#[spacetimedb::table(accessor = idc_noop_outbox, outbox(receiver::noop))]
pub struct IdcNoopOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
}

#[spacetimedb::reducer]
pub fn enqueue_idc(ctx: &ReducerContext, target: receiver::Identity, message: String) {
    ctx.db.idc_outbox().insert(IdcOutbox {
        id: 0,
        tag: "example".to_owned(),
        target,
        message,
    });
    ctx.db.idc_noop_outbox().insert(IdcNoopOutbox { id: 0, target });
}

#[spacetimedb::reducer(on_result(idc_outbox))]
pub fn on_idc_result(_ctx: &ReducerContext, row: IdcOutbox, result: Result<(), String>) {
    if let Err(error) = result {
        spacetimedb::log::warn!("IDC message {} ({}) failed: {error}", row.id, row.tag);
    }
}
