#[path = "../fixtures/idc_receiver/lib.rs"]
mod receiver;

mod remote_bindings {
    pub(crate) use crate::receiver;
}

#[spacetimedb::table(accessor = outbound_ping, name = "WireOutbox", outbox(crate::remote_bindings::receiver::receive_ping))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    audit: String,
    #[param(name = body)]
    payload: String,
    #[target]
    target: receiver::Identity,
}

mod outboxes {
    use super::receiver;

    #[spacetimedb::table(accessor = plain_ping, outbox(self::receiver::receive_ping))]
    pub struct PlainPing {
        #[primary_key]
        #[auto_inc]
        id: u64,
        #[target]
        target: receiver::Identity,
        body: String,
    }
}

#[spacetimedb::table(accessor = empty_ping, outbox(receiver::noop))]
pub struct EmptyPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
}

#[spacetimedb::reducer(on_result(self::outbound_ping))]
pub fn on_ping_result(_: &spacetimedb::ReducerContext, _: OutboundPing, _: Result<(), String>) {}

mod callbacks {
    #[spacetimedb::reducer(name = "WireCallback", on_result(super::outboxes::plain_ping))]
    pub fn on_plain_result(_: &spacetimedb::ReducerContext, _: super::outboxes::PlainPing, _: Result<(), String>) {}
}

fn main() {
    use spacetimedb::rt::FnInfo;
    use spacetimedb::table::TableInternal;

    assert_eq!(on_ping_result::ON_RESULT_OUTBOX, Some("outbound_ping"));
    assert_eq!(callbacks::on_plain_result::ON_RESULT_OUTBOX, Some("plain_ping"));
    assert_eq!(outbound_ping__TableHandle::OUTBOX.unwrap().arg_columns, &[3, 2]);
    assert_eq!(outboxes::plain_ping__TableHandle::OUTBOX.unwrap().arg_columns, &[1, 2]);
    assert!(empty_ping__TableHandle::OUTBOX.unwrap().arg_columns.is_empty());
    assert_eq!(
        outboxes::plain_ping__TableHandle::OUTBOX.unwrap().remote_reducer_name,
        "wire_ping"
    );
}

// Outbox tables retain inserts and reads, including primary-key lookups.
fn read_and_insert(ctx: &spacetimedb::ReducerContext, row: OutboundPing) {
    use spacetimedb::Table;
    let table = ctx.db.outbound_ping();
    let row = table.insert(row);
    let _ = table.id().find(row.id);
    let _ = table.try_insert(row);
    let _ = table.count();
    let _ = table.iter();
}

#[spacetimedb::table(
    accessor = ordinary,
    index(accessor = by_value, btree(columns = [value])),
    index(accessor = by_value_hash, hash(columns = [value])),
)]
pub struct Ordinary {
    #[primary_key]
    id: u64,
    value: String,
}

// Ordinary tables retain all update and delete APIs.
fn mutate_ordinary(ctx: &spacetimedb::ReducerContext, row: Ordinary) {
    use spacetimedb::Table;
    let table = ctx.db.ordinary();
    let row = table.id().update(row);
    #[cfg(feature = "unstable")]
    let row = table.id().insert_or_update(row);
    #[cfg(feature = "unstable")]
    let row = table.id().try_insert_or_update(row).unwrap();
    table.delete(row);
    table.id().delete(1);
    table.by_value().delete("value");
    table.by_value_hash().delete("value");
    table.clear();
}
