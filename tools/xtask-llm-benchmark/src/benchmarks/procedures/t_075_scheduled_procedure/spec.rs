use crate::eval::defaults::{default_schema_parity_scorers, make_eventually_sql_count_scorer, make_reducer_call_both_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url, file!(), route_tag, "schedule_procedure", vec![json!(9), json!(7), json!(5)], "schedule_procedure",
        ));
        let table = table_name("procedure_result", lang);
        let id = ident("id", casing_for_lang(lang));
        let value = ident("value", casing_for_lang(lang));
        scorers.push(make_eventually_sql_count_scorer(
            host_url, file!(), route_tag, format!("SELECT COUNT(*) AS n FROM {table} WHERE {id}=9 AND {value}=12"),
            1, "scheduled_procedure_completes", Duration::from_secs(10),
        ));
        scorers
    })
}
