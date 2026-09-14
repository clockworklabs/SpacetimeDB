use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer_with_attempts, make_sql_count_only_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer_with_attempts(
            host_url,
            file!(),
            route_tag,
            "fetch_and_store",
            vec![json!("https://example.com")],
            "fetch_and_store",
            3,
        ));
        let table = table_name("fetched_record", lang);
        let status = ident("status", casing_for_lang(lang));
        let valid = ident("valid_body", casing_for_lang(lang));
        scorers.push(make_sql_count_only_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {status}=200 AND {valid}=true"),
            1,
            "fetched_row_stored",
            Duration::from_secs(10),
        ));
        scorers
    })
}
