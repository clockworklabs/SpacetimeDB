use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{casing_for_lang, table_name, BenchmarkSpec, ReducerDataParityConfig, SqlBuilder};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let sql = SqlBuilder::new(casing_for_lang(lang));
        let columns = sql.cols(&["category", "total_amount", "sale_count"]).join(", ");
        scorers.push(make_reducer_data_parity_scorer(host_url, ReducerDataParityConfig {
            src_file: file!(), route_tag, reducer: "exercise".into(), args: vec![],
            select_query: format!("SELECT {columns} FROM {}", table_name("category_summary", lang)),
            id_str: "aggregate_is_synchronized", collapse_ws: true, timeout: Duration::from_secs(10),
        }));
        scorers
    })
}
