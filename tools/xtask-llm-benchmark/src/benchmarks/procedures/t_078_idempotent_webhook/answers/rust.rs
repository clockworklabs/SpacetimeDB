use spacetimedb::{table, Table};
use spacetimedb::http::{handler, router, Body, HandlerContext, Request, Response, Router};

#[table(accessor = processed_event)]
pub struct ProcessedEvent { #[primary_key] pub event_id: String }

#[table(accessor = webhook_state, public)]
pub struct WebhookState { #[primary_key] pub key: String, pub last_sequence: u64, pub value: String }

#[handler]
fn webhook(ctx: &mut HandlerContext, request: Request) -> Response {
    let body = request.into_body().into_string_lossy();
    let parts: Vec<_> = body.splitn(3, '|').collect();
    if parts.len() != 3 { return Response::builder().status(400).body(Body::from_bytes("invalid")).unwrap(); }
    let event_id = parts[0].to_string();
    let sequence: u64 = parts[1].parse().expect("invalid sequence");
    let value = parts[2].to_string();
    let outcome = ctx.with_tx(|tx| {
        if tx.db.processed_event().event_id().find(&event_id).is_some() { return "duplicate"; }
        tx.db.processed_event().insert(ProcessedEvent { event_id: event_id.clone() });
        let key = "account".to_string();
        if let Some(mut state) = tx.db.webhook_state().key().find(&key) {
            if sequence <= state.last_sequence { return "stale"; }
            state.last_sequence = sequence; state.value = value.clone(); tx.db.webhook_state().key().update(state);
        } else {
            tx.db.webhook_state().insert(WebhookState { key, last_sequence: sequence, value: value.clone() });
        }
        "applied"
    });
    Response::new(Body::from_bytes(outcome))
}

#[router]
fn routes() -> Router { Router::new().post("/webhook", webhook) }
