use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, ident, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use serde_json::json;
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let sql = SqlBuilder::new(casing_for_lang(lang));
        let table = table_name("command_result", lang);
        let columns = sql.cols(&["request_id", "success", "message"]).join(", ");
        let request_id = ident("request_id", casing_for_lang(lang));
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "run_command".into(),
                args: vec![json!("req-1"), json!(7)],
                select_query: format!("SELECT {columns} FROM {table} WHERE {request_id}='req-1'"),
                id_str: "result_row_written",
                collapse_ws: true,
                timeout: Duration::from_secs(10),
            },
        ));
        scorers
    })
}
