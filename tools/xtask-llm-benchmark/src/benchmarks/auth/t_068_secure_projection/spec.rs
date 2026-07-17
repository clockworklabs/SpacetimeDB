use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let sql = SqlBuilder::new(casing_for_lang(lang));
        let columns = sql.cols(&["id", "title"]).join(", ");
        scorers.push(make_reducer_data_parity_scorer(host_url, ReducerDataParityConfig {
            src_file: file!(), route_tag, reducer: "seed_private_note".into(), args: vec![],
            select_query: format!("SELECT {columns} FROM {}", table_name("my_safe_note", lang)),
            id_str: "caller_safe_projection", collapse_ws: true, timeout: Duration::from_secs(10),
        }));
        scorers
    })
}
