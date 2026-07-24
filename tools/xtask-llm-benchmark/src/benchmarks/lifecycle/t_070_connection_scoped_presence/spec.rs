use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_call_both_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "exercise_presence",
            vec![],
            "exercise_presence",
        ));
        let table = table_name("presence_session", lang);
        let connection_id = ident("connection_id", casing_for_lang(lang));
        for (value, expected, id) in [(1, 0, "first_connection_removed"), (2, 1, "second_connection_retained")] {
            scorers.push(make_sql_count_only_scorer(
                host_url,
                file!(),
                route_tag,
                format!("SELECT COUNT(*) AS n FROM {table} WHERE {connection_id}=0x{value:032x}"),
                expected,
                id,
                Duration::from_secs(10),
            ));
        }
        scorers
    })
}
