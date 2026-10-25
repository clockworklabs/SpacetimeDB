#![allow(non_snake_case)]

#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: spacetimedb::CaseConversionPolicy = spacetimedb::CaseConversionPolicy::None;

#[spacetimedb::reducer]
pub fn receivePing(_: &spacetimedb::ReducerContext, messageBody: String) {}

#[spacetimedb::table(accessor = outboundPing, outbox(self::receivePing))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: spacetimedb::Identity,
    #[param(name = messageBody)]
    payload: String,
}

mod nested {
    #[spacetimedb::table(accessor = outbound_nested, outbox(super::receivePing))]
    pub struct OutboundNested {
        #[primary_key]
        #[auto_inc]
        id: u64,
        #[target]
        target: spacetimedb::Identity,
        #[param(name = messageBody)]
        payload: String,
    }
}

fn main() {
    use spacetimedb::table::TableInternal;
    assert_eq!(outboundPing__TableHandle::OUTBOX.unwrap().arg_columns, &[2]);
    assert_eq!(
        nested::outbound_nested__TableHandle::OUTBOX.unwrap().arg_columns,
        &[2]
    );
    assert_eq!(<receivePing as spacetimedb::rt::OutboxReceiver>::SIGNATURE_HASH, None);
}
