use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{table_name, BenchmarkSpec, ReducerDataParityConfig};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "seed".into(),
                args: vec![],
                select_query: format!("SELECT * FROM {}", table_name("source_view", lang)),
                collapse_ws: true,
                timeout: Duration::from_secs(10),
                id_str: "visible_rows_only",
            },
        ));
        scorers
    })
}
