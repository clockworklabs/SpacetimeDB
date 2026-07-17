use crate::eval::defaults::{default_schema_parity_scorers, make_http_route_parity_scorer};
use crate::eval::BenchmarkSpec;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |_lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_http_route_parity_scorer(
            host_url, file!(), route_tag,
            vec![
                ("GET", "/items", None),
                ("POST", "/items", Some("book")),
                ("PUT", "/items", Some("no")),
                ("GET", "/missing", None),
            ],
            false,
            "router_method_matrix",
        ));
        scorers
    })
}
