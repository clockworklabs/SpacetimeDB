use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_sql_count_scorer};
use crate::eval::{table_name, BenchmarkSpec, ReducerSqlCountConfig};
use std::time::Duration;
pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_sql_count_scorer(host_url, ReducerSqlCountConfig {
            src_file: file!(), route_tag, reducer: "seed".into(), args: vec![],
            sql_count_query: format!("SELECT COUNT(*) AS n FROM {}", table_name("order_line_detail", lang)),
            expected_count: 1, id_str: "three_table_join", timeout: Duration::from_secs(10),
        })); scorers
    })
}
