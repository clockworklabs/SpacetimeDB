use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_sql_count_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerSqlCountConfig};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let casing = casing_for_lang(lang);
        let parent = table_name("parent", lang);
        let child = table_name("child", lang);
        let parent_id = ident("parent_id", casing);
        let id = ident("id", casing);
        let name = ident("name", casing);
        let sql = format!(
            "SELECT COUNT(*) AS n FROM {child} JOIN {parent} ON {child}.{parent_id}={parent}.{id} WHERE {parent}.{name}='Taylor'"
        );

        scorers.push(make_reducer_sql_count_scorer(
            host_url,
            ReducerSqlCountConfig {
                src_file: file!(),
                route_tag,
                reducer: "create_family".into(),
                args: vec![json!("Taylor"), json!(["Avery", "Jordan"])],
                sql_count_query: sql,
                expected_count: 2,
                id_str: "children_reference_inserted_parent",
                timeout: Duration::from_secs(10),
            },
        ));
        scorers
    })
}
