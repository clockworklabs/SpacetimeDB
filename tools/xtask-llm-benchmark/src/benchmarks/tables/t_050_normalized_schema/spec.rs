use crate::eval::defaults::default_schema_parity_scorers;
use crate::eval::BenchmarkSpec;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |_lang, route_tag, host_url| {
        default_schema_parity_scorers(host_url, file!(), route_tag)
    })
}
