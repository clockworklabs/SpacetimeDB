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
