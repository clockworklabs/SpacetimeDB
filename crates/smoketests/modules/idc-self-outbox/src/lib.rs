#![allow(non_snake_case)]

use spacetimedb::Table;

#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: spacetimedb::CaseConversionPolicy = spacetimedb::CaseConversionPolicy::None;

#[derive(spacetimedb::SpacetimeType)]
pub struct Message {
    pub ItemID: u64,
    pub operations: Vec<Operation>,
}

#[derive(spacetimedb::SpacetimeType)]
pub enum Operation {
    SetState(State),
    ClearState,
}

#[derive(spacetimedb::SpacetimeType)]
pub struct State {
    pub revision: u64,
    pub displayName: Option<String>,
}

#[spacetimedb::table(accessor = delivered_ping, name = "delivered_ping")]
pub struct DeliveredPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: Message,
}

#[spacetimedb::reducer]
pub fn receivePing(ctx: &spacetimedb::ReducerContext, messageBody: Message) {
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
    payload: Message,
}

#[spacetimedb::table(accessor = ping_results)]
pub struct PingResult {
    #[primary_key]
    id: u64,
    body: Message,
    error: Option<String>,
}

#[spacetimedb::reducer(name = "WirePingResult", on_result(self::outboundPing))]
pub fn onPingResult(ctx: &spacetimedb::ReducerContext, row: OutboundPing, result: Result<(), String>) {
    ctx.db.ping_results().insert(PingResult {
        id: row.id,
        body: row.payload,
        error: result.err(),
    });
}

#[spacetimedb::reducer]
pub fn enqueuePing(ctx: &spacetimedb::ReducerContext, target: spacetimedb::Identity, messageBody: Message) {
    ctx.db.outboundPing().insert(OutboundPing {
        id: 0,
        target,
        payload: messageBody,
    });
}
