use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, SqlBuilder};
use serde_json::Value;
use std::time::Duration;


pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);

        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let reducer = ident("ComputeSum", casing);
        let select = sb.select_by_id("results", &["id","sum"], "id", 1);

        v.push(make_reducer_data_parity_scorer(
            file!(),
            route_tag,
            reducer,
            vec![Value::from(1), Value::from(2), Value::from(3)],
            &select,
            "helper_func_sum_parity",
            true,
            Duration::from_secs(10),
        ));

        let id = sb.cols(&["id"])[0].clone();
        let sum = sb.cols(&["sum"])[0].clone();
        let q = format!("SELECT COUNT(*) AS n FROM results WHERE {id}=1 AND {sum}=5");

        v.push(make_sql_count_only_scorer(
            file!(),
            route_tag,
            q,
            1,
            "helper_func_sum_abs",
            Duration::from_secs(10),
        ));

        v
    })
}