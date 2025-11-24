use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, ident, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::Value;
use std::time;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag| {
        let mut v = default_schema_parity_scorers(file!(), route_tag);
        let casing = casing_for_lang(lang);
        let sb = SqlBuilder::new(casing);
        let select = sb.select_by_id("users", &["id","name","age","active"], "id", 1);
        let reducer_name = ident("InsertUser", casing);

        v.push(make_reducer_data_parity_scorer(ReducerDataParityConfig {
            src_file: file!(),
            route_tag,
            reducer: reducer_name.into(),
            args: vec![
                Value::from(1),
                Value::from("Alice"),
                Value::from(30),
                Value::from(true),
            ],
            select_query: select.clone(),
            id_str: "data_parity_insert_user",
            collapse_ws: true,
            timeout: time::Duration::from_secs(10),
        }));
        v
    })
}
