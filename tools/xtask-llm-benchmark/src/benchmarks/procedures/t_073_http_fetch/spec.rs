use crate::eval::defaults::{default_schema_parity_scorers, make_call_output_parity_scorer};
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |_lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_call_output_parity_scorer(
            host_url, file!(), route_tag, "fetch_schema_summary", vec![json!(host_url)], "http_response_summary",
        ));
        scorers
    })
}
