use crate::eval::{client::reconnect_client, BenchmarkSpec};
pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route, host| {
        vec![reconnect_client(file!(), route, host, lang)]
    })
}
