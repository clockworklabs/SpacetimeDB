use crate::eval::defaults::{default_schema_parity_scorers, make_call_output_parity_scorer};
use crate::eval::BenchmarkSpec;
use serde_json::json;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |_lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_call_output_parity_scorer(
            host_url,
            file!(),
            route_tag,
            "calculate_summary",
            vec![json!(7), json!(5)],
            "typed_procedure_return",
        ));
        scorers
    })
}
