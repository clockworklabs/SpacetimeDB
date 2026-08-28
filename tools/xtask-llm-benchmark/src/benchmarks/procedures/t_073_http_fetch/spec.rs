use crate::eval::defaults::{default_schema_parity_scorers, make_call_output_parity_scorer_with_attempts};
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |_lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_call_output_parity_scorer_with_attempts(
            host_url,
            file!(),
            route_tag,
            "fetch_page_summary",
            vec![json!("https://example.com")],
            "http_response_summary",
            3,
        ));
        scorers
    })
}
