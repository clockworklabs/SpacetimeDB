use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::BenchmarkSpec;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |_lang, route_tag| {
        default_schema_parity_scorers(file!(), route_tag)
    })
}
