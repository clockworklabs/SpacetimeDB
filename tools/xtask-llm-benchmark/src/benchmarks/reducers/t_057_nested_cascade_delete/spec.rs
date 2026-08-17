use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_call_both_scorer, make_sql_count_only_scorer};
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
        let id = ident("id", casing_for_lang(lang));
        for (table, count_id, preserved_id) in [
            ("workspace", "workspace_count", "workspace_two_preserved"),
            ("project", "project_count", "project_two_preserved"),
            ("task_item", "task_count", "task_two_preserved"),
            ("task_note", "note_count", "note_two_preserved"),
        ] {
            let table = table_name(table, lang);
            scorers.push(make_sql_count_only_scorer(
                host_url,
                file!(),
                route_tag,
                format!("SELECT COUNT(*) AS n FROM {table}"),
                1,
                count_id,
                Duration::from_secs(10),
            ));
            scorers.push(make_sql_count_only_scorer(
                host_url,
                file!(),
                route_tag,
                format!("SELECT COUNT(*) AS n FROM {table} WHERE {id}=2"),
                1,
                preserved_id,
                Duration::from_secs(10),
            ));
        }
        scorers
    })
}
