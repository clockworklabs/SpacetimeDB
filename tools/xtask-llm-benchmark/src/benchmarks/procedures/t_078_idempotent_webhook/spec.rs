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
        scorers
    })
}
