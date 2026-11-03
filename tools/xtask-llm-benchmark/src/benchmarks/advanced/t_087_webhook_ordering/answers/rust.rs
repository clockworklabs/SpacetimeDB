use spacetimedb::http::{handler, router, Body, HandlerContext, Request, Response, Router};
use spacetimedb::{table, Table};
#[table(accessor = webhook_receipt, public)]
pub struct WebhookReceipt {
    #[primary_key]
    pub event_id: String,
    pub account: String,
    pub sequence: u64,
    pub value: String,
}

#[table(accessor = webhook_account, public)]
pub struct WebhookAccount {
    #[primary_key]
    pub account: String,
    pub sequence: u64,
    pub value: String,
}

#[handler]
pub fn webhook(ctx: &mut HandlerContext, request: Request) -> Response {
    let body = request.into_body().into_string_lossy();
    let p: Vec<_> = body.split('|').collect();
    let invalid = || {
        Response::builder()
            .status(400)
            .body(Body::from_bytes("invalid"))
            .unwrap()
    };
    if p.len() != 4 || p.iter().any(|s| s.is_empty()) || !p[2].bytes().all(|c| c.is_ascii_digit()) {
        return invalid();
    }
    let Ok(sequence) = p[2].parse::<u64>() else {
        return invalid();
    };
    if sequence == 0 {
        return invalid();
    }
    let (account, event_id, value) = (p[0].to_string(), p[1].to_string(), p[3].to_string());
    let result = ctx.with_tx(|tx| {
        if let Some(old) = tx.db.webhook_receipt().event_id().find(&event_id) {
            return if old.account == account && old.sequence == sequence && old.value == value {
                "duplicate"
            } else {
                "conflict"
            };
        }
        tx.db.webhook_receipt().insert(WebhookReceipt {
            event_id: event_id.clone(),
            account: account.clone(),
            sequence,
            value: value.clone(),
        });
        if let Some(old) = tx.db.webhook_account().account().find(&account) {
            if sequence <= old.sequence {
                return "stale";
            }
            tx.db.webhook_account().account().update(WebhookAccount {
                account: account.clone(),
                sequence,
                value: value.clone(),
            });
        } else {
            tx.db.webhook_account().insert(WebhookAccount {
                account: account.clone(),
                sequence,
                value: value.clone(),
            });
        }
        "applied"
    });
    Response::new(Body::from_bytes(result))
}
#[router]
pub fn routes() -> Router {
    Router::new().post("/webhook", webhook)
}
