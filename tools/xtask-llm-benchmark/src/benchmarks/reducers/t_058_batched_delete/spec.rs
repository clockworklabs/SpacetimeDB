use crate::eval::defaults::{
    default_schema_parity_scorers, make_eventually_sql_count_scorer, make_reducer_call_both_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "seed_group",
            vec![json!(7), json!(5)],
            "seed_group",
        ));
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "request_delete",
            vec![json!(7)],
            "request_delete",
        ));
        let table = table_name("work_item", lang);
        let group_id = ident("group_id", casing_for_lang(lang));
        scorers.push(make_eventually_sql_count_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT COUNT(*) AS n FROM {table} WHERE {group_id}=7"),
            0,
            "scheduled_batches_finish",
            Duration::from_secs(10),
        ));
        scorers
    })
}
