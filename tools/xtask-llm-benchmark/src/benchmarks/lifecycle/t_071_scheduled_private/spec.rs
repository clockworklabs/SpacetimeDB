use crate::eval::defaults::{default_schema_parity_scorers, make_eventually_sql_count_scorer, make_reducer_call_both_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url, file!(), route_tag, "enqueue_private", vec![json!(7)], "enqueue_private",
        ));
        let table = table_name("job_result", lang);
        let id = ident("id", casing_for_lang(lang));
        let status = ident("status", casing_for_lang(lang));
        scorers.push(make_eventually_sql_count_scorer(
            host_url, file!(), route_tag,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {id}=7 AND {status}='complete'"),
            1, "private_scheduled_job_completes", Duration::from_secs(10),
        ));
        scorers
    })
}
