use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let sql = SqlBuilder::new(casing_for_lang(lang));
        let table = table_name("blob_record", lang);
        let columns = sql.cols(&["filename", "mime_type", "size", "data"]).join(", ");
        let filename = ident("filename", casing_for_lang(lang));
        let select_query = format!("SELECT {columns} FROM {table} WHERE {filename}='eval.bin'");

        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "store_blob".into(),
                args: vec![
                    json!("eval.bin"),
                    json!("application/octet-stream"),
                    json!([0, 1, 2, 127, 128, 255]),
                ],
                select_query,
                id_str: "binary_round_trip",
                collapse_ws: true,
                timeout: Duration::from_secs(10),
            },
        ));
        scorers
    })
}
