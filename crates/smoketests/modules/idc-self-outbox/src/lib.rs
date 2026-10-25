#![allow(non_snake_case)]

use spacetimedb::Table;

#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: spacetimedb::CaseConversionPolicy = spacetimedb::CaseConversionPolicy::None;

#[spacetimedb::table(accessor = delivered_ping, name = "delivered_ping")]
pub struct DeliveredPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: String,
}

#[spacetimedb::reducer]
pub fn receivePing(ctx: &spacetimedb::ReducerContext, messageBody: String) {
    ctx.db.delivered_ping().insert(DeliveredPing {
        id: 0,
        body: messageBody,
    });
}

#[spacetimedb::table(accessor = outboundPing, name = "outbound_ping", outbox(crate::receivePing))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: spacetimedb::Identity,
    #[param(name = messageBody)]
    payload: String,
}

#[spacetimedb::reducer]
pub fn enqueuePing(ctx: &spacetimedb::ReducerContext, target: spacetimedb::Identity, messageBody: String) {
    ctx.db.outboundPing().insert(OutboundPing {
        id: 0,
        target,
        payload: messageBody,
    });
}
