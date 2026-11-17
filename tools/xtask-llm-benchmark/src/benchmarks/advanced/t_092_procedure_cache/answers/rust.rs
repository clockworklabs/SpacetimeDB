use spacetimedb::{procedure, ProcedureContext};
use spacetimedb::{table, Table};
#[table(accessor = cache_entry, public)]
pub struct CacheEntry {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub product: String,
    pub language: String,
    pub value: String,
    pub expires_at: i64,
}

#[procedure]
pub fn fetch_cached(ctx: &mut ProcedureContext, product: String, language: String, ttl_ms: u64, url: String) -> String {
    if product.is_empty() || language.is_empty() || ttl_ms > 60000 {
        return "invalid input".into();
    }
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    if ttl_ms > 0 {
        let cached = ctx.with_tx(|tx| {
            tx.db
                .cache_entry()
                .iter()
                .find(|r| r.product == product && r.language == language && r.expires_at > now)
        });
        if let Some(row) = cached {
            return row.value;
        }
    }
    let Ok(response) = ctx.http.get(url) else {
        return "upstream error".into();
    };
    if response.status().as_u16() != 200 {
        return "upstream error".into();
    }
    let value = response.into_body().into_string_lossy();
    if ttl_ms > 0 {
        ctx.with_tx(|tx| {
            let old = tx
                .db
                .cache_entry()
                .iter()
                .find(|r| r.product == product && r.language == language);
            let next = CacheEntry {
                id: old.as_ref().map_or(0, |r| r.id),
                product: product.clone(),
                language: language.clone(),
                value: value.clone(),
                expires_at: tx.timestamp.to_micros_since_unix_epoch() + ttl_ms as i64 * 1000,
            };
            if old.is_some() {
                tx.db.cache_entry().id().update(next);
            } else {
                tx.db.cache_entry().insert(next);
            }
        });
    }
    value
}
