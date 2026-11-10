use spacetimedb::Table;

#[path = "../fixtures/idc_receiver/lib.rs"]
mod receiver;

#[spacetimedb::table(
    accessor = outbound_ping,
    outbox(receiver::receive_ping),
    index(accessor = by_body, btree(columns = [body])),
    index(accessor = by_body_hash, hash(columns = [body])),
)]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
    body: String,
}

fn mutate(ctx: &spacetimedb::ReducerContext, row: OutboundPing) {
    let table = ctx.db.outbound_ping();
    table.clear();
    table.id().delete(1);
    table.by_body().delete("body");
    table.by_body_hash().delete("body");
    table.id().update(row);
}

fn delete(ctx: &spacetimedb::ReducerContext, row: OutboundPing) {
    ctx.db.outbound_ping().delete(row);
}

fn upsert(ctx: &spacetimedb::ReducerContext, row: OutboundPing) {
    ctx.db.outbound_ping().id().insert_or_update(row);
}

fn try_upsert(ctx: &spacetimedb::ReducerContext, row: OutboundPing) {
    let _ = ctx.db.outbound_ping().id().try_insert_or_update(row);
}

fn main() {}
