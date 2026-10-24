#![allow(non_snake_case)]

#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: spacetimedb::CaseConversionPolicy = spacetimedb::CaseConversionPolicy::None;

#[spacetimedb::reducer]
pub fn receivePing(_: &spacetimedb::ReducerContext, messageBody: String) {}

#[spacetimedb::table(accessor = outboundPing, outbox(local = crate::receivePing))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: spacetimedb::Identity,
    #[param(name = messageBody)]
    payload: String,
}

fn main() {
    use spacetimedb::table::TableInternal;
    assert_eq!(outboundPing__TableHandle::OUTBOX.unwrap().arg_columns, &[2]);
}
