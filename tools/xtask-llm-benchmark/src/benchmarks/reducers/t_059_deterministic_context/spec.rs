use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_sql_count_scorer, make_sql_distinct_rows_scorer,
    make_sql_output_excludes_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerSqlCountConfig};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let table = table_name("generated_value", lang);
        let casing = casing_for_lang(lang);
        scorers.push(make_reducer_sql_count_scorer(
            host_url,
            ReducerSqlCountConfig {
                src_file: file!(),
                route_tag,
                reducer: "generate".into(),
                args: vec![],
                sql_count_query: format!("SELECT COUNT(*) AS n FROM {table}"),
                expected_count: 1,
                id_str: "generated_value_recorded",
                timeout: Duration::from_secs(10),
            },
        ));
        scorers.push(make_reducer_sql_count_scorer(
            host_url,
            ReducerSqlCountConfig {
                src_file: file!(),
                route_tag,
                reducer: "generate".into(),
                args: vec![],
                sql_count_query: format!("SELECT COUNT(*) AS n FROM {table}"),
                expected_count: 2,
                id_str: "second_generated_value_recorded",
                timeout: Duration::from_secs(10),
            },
        ));
        let random_value = ident("random_value", casing);
        scorers.push(make_sql_distinct_rows_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT {random_value} FROM {table}"),
            2,
            "successive_random_values_differ",
            Duration::from_secs(10),
        ));
        let created_at = ident("created_at", casing);
        scorers.push(make_sql_output_excludes_scorer(
            host_url,
            file!(),
            route_tag,
            format!("SELECT {created_at} FROM {table}"),
            vec!["1970-01-01T00:00:00+00:00".into()],
            "context_timestamp_recorded",
        ));
        scorers
    })
}
