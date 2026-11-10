use crate::eval::defaults::{default_schema_parity_scorers, make_http_route_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_http_route_parity_scorer(
            host_url,
            file!(),
            route_tag,
            vec![
                ("POST", "/webhook", Some("evt-1|2|new")),
                ("POST", "/webhook", Some("evt-1|2|new")),
                ("POST", "/webhook", Some("evt-2|1|old")),
            ],
            false,
            "webhook_idempotency",
        ));
        let table = table_name("webhook_state", lang);
        let key = ident("key", casing_for_lang(lang));
        let sequence = ident("last_sequence", casing_for_lang(lang));
        let value = ident("value", casing_for_lang(lang));
        scorers.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {key}='account' AND {sequence}=2 AND {value}='new'"),
            1,
            "webhook_state_is_current",
            Duration::from_secs(10),
        ));
        scorers.push(crate::eval::scenario::scenario(
            file!(),
            route_tag,
            host_url,
            lang,
            |s| {
                // This older task defines all repeated event IDs as no-ops, even with a changed payload.
                s.expect_http("/webhook", "evt-1|99|changed", 200, "duplicate")?;
                s.rows(
                    "webhook_state",
                    &["key", "last_sequence", "value"],
                    serde_json::json!([["account", 2, "new"]]),
                )?;
                s.expect_http("/webhook", "evt-3|3|latest", 200, "applied")?;
                s.expect_http("/webhook", "evt-4|2|stale", 200, "stale")?;
                s.rows(
                    "webhook_state",
                    &["key", "last_sequence", "value"],
                    serde_json::json!([["account", 3, "latest"]]),
                )?;
                Ok(())
            },
        ));
        scorers
    })
}
