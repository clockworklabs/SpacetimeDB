use crate::eval::defaults::{
    default_schema_parity_scorers, make_reducer_call_both_scorer, make_reducer_data_parity_scorer,
};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_call_both_scorer(
            host_url,
            file!(),
            route_tag,
            "create_profile",
            vec![json!(1), json!("light"), json!(true), json!("UTC")],
            "create_profile",
        ));
        let sql = SqlBuilder::new(casing_for_lang(lang));
        let table = table_name("profile", lang);
        let columns = sql.cols(&["id", "preferences"]).join(", ");
        let id = ident("id", casing_for_lang(lang));
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "update_theme".into(),
                args: vec![json!(1), json!("dark")],
                select_query: format!("SELECT {columns} FROM {table} WHERE {id}=1"),
                id_str: "nested_siblings_preserved",
                collapse_ws: true,
                timeout: Duration::from_secs(10),
            },
        ));
        scorers
    })
}
