use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_sql_count_only_scorer,
};
use crate::eval::{table_name, BenchmarkSpec};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "seed",
            vec![],
            "seed_nested_rows",
        ));
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "delete_workspace",
            vec![json!(1)],
            "delete_workspace",
        ));
        for (table, scorer_id) in [
            ("workspace", "workspace_count"),
            ("project", "project_count"),
            ("task_item", "task_count"),
            ("task_note", "note_count"),
        ] {
            scorers.push(make_sql_count_only_scorer(
                host_url,
                file!(),
                route_tag,
                format!("SELECT COUNT(*) AS n FROM {}", table_name(table, lang)),
                1,
                scorer_id,
                Duration::from_secs(10),
            ));
        }
        scorers
    })
}
