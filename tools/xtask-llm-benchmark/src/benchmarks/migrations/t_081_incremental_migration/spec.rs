use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let casing = casing_for_lang(lang);
        let item_v2 = table_name("item_v2", lang);
        let id = ident("id", casing);
        let value = ident("value", casing);
        let version = ident("version", casing);
        let v2_query = format!("SELECT {id}, {value}, {version} FROM {item_v2}");
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "migrate".into(),
                args: vec![],
                select_query: v2_query.clone(),
                collapse_ws: true,
                timeout: Duration::from_secs(10),
                id_str: "legacy_rows_migrated",
            },
        ));
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "dual_write".into(),
                args: vec![json!(2), json!("new")],
                select_query: v2_query,
                collapse_ws: true,
                timeout: Duration::from_secs(10),
                id_str: "dual_write_keeps_v2_current",
            },
        ));
        let legacy = table_name("legacy_item", lang);
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "migrate".into(),
                args: vec![],
                select_query: format!("SELECT {id}, {value} FROM {legacy}"),
                collapse_ws: true,
                timeout: Duration::from_secs(10),
                id_str: "legacy_table_remains_current",
            },
        ));
        scorers
    })
}
