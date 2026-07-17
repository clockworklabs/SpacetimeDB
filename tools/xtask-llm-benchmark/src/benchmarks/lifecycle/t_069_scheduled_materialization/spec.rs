use crate::eval::defaults::{default_schema_parity_scorers, make_eventually_sql_count_scorer, make_reducer_call_both_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(host_url, file!(), route_tag, "start_refresh", vec![], "start_refresh"));
        let table = table_name("materialized_state", lang);
        let status = ident("status", casing_for_lang(lang));
        let version = ident("version", casing_for_lang(lang));
        scorers.push(make_eventually_sql_count_scorer(
            host_url, file!(), route_tag,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {status}='ready' AND {version}=1"),
            1, "scheduled_refresh_completes", Duration::from_secs(10),
        ));
        scorers
    })
}
