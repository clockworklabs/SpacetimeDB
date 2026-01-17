use crate::eval::defaults::{
    default_schema_parity_scorers,
    make_reducer_sql_count_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerSqlCountConfig, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut v = default_schema_parity_scorers(host_url, file!(), route_tag);

        let case = casing_for_lang(lang);
        let sb   = SqlBuilder::new(case);

        let seed = ident("Seed", case);
        let log_table = table_name("log", lang);

        let user_id = ident("user_id", sb.case);
        let day     = ident("day", sb.case);

        let base = |reducer: &str| ReducerSqlCountConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer.to_string(),
            args: vec![],
            sql_count_query: String::new(), // override per case
            expected_count: 0,              // override per case
            id_str: "",
            timeout: Duration::from_secs(10),
        };

        v.push(make_reducer_sql_count_scorer(host_url, ReducerSqlCountConfig {
            sql_count_query: format!("SELECT COUNT(*) AS n FROM {log_table}"),
            expected_count: 3,
            id_str: "mcindex_seed_count",
            ..base(&seed)
        }));

        v.push(make_reducer_sql_count_scorer(host_url, ReducerSqlCountConfig {
            sql_count_query: format!(
                "SELECT COUNT(*) AS n FROM {log_table} WHERE {u}=7 AND {d}=1",
                u = user_id, d = day
            ),
            expected_count: 1,
            id_str: "mcindex_lookup_u7_d1",
            ..base(&seed)
        }));

        v.push(make_reducer_sql_count_scorer(host_url, ReducerSqlCountConfig {
            sql_count_query: format!(
                "SELECT COUNT(*) AS n FROM {log_table} WHERE {u}=7 AND {d}=2",
                u = user_id, d = day
            ),
            expected_count: 1,
            id_str: "mcindex_lookup_u7_d2",
            ..base(&seed)
        }));

        v
    })
}
