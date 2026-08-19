use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let counter = table_name("counter", lang);
        let casing = casing_for_lang(lang);
        let id = ident("id", casing);
        let value = ident("value", casing);
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "increment".into(),
                args: vec![json!(1), json!(2)],
                select_query: format!("SELECT {id}, {value} FROM {counter}"),
                collapse_ws: true,
                timeout: Duration::from_secs(10),
                id_str: "existing_api_survives_republish",
            },
        ));
        scorers
    })
}
