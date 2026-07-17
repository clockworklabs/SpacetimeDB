use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let table = table_name("user_record", lang);
        let name = ident("name", casing_for_lang(lang));
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "register_self".into(),
                args: vec![json!("caller")],
                select_query: format!("SELECT {name} FROM {table}"),
                collapse_ws: true,
                timeout: Duration::from_secs(10),
                id_str: "caller_sees_own_row",
            },
        ));
        scorers
    })
}
