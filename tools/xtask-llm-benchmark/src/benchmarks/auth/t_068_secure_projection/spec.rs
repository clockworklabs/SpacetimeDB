use crate::eval::defaults::{default_schema_parity_scorers, make_reducer_data_parity_scorer};
use crate::eval::{table_name, BenchmarkSpec, ReducerDataParityConfig};
use std::time::Duration;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        let view = table_name("my_safe_note", lang);
        scorers.push(make_reducer_data_parity_scorer(
            host_url,
            ReducerDataParityConfig {
                src_file: file!(),
                route_tag,
                reducer: "seed_private_note".into(),
                args: vec![],
                select_query: format!("SELECT * FROM {view}"),
                id_str: "caller_safe_projection",
                collapse_ws: true,
                timeout: Duration::from_secs(10),
            },
        ));
        scorers
    })
}
