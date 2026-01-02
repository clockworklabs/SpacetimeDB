use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer, make_sql_count_only_scorer};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::Value;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);
        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing_for_lang(lang));
        let reducer = ident("SetCircle", casing);

        let select = sb.select_by_id("results", &["id","value"], "id", 1);
        v.push(make_reducer_data_parity_scorer(ReducerDataParityConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer.into(),
            args: vec![
                Value::from(1),
                Value::from(10),
            ],
            select_query: select.clone(),
            id_str: "sum_type_row_parity",
            collapse_ws: true,
            timeout: Duration::from_secs(10),
        }));


        let count = sb.count_by_id("results", "id", 1);
        v.push(make_sql_count_only_scorer(
            file!(),
            route_tag,
            &count,
            1,
            "sum_type_row_count",
            Duration::from_secs(10),
        ));

        v
    })
}