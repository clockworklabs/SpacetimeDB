use crate::eval::defaults::{default_schema_parity_scorers, make_http_route_parity_scorer};
use crate::eval::BenchmarkSpec;

pub fn spec() -> BenchmarkSpec {
    BenchmarkSpec::from_tasks_auto(file!(), |lang, route_tag, host_url| {
        let mut scorers = default_schema_parity_scorers(host_url, file!(), route_tag);
        scorers.push(make_http_route_parity_scorer(
            host_url,
            file!(),
            route_tag,
            vec![("GET", "/items", None), ("POST", "/items", Some("book"))],
            false,
            "router_method_matrix",
        ));
        // The prompt says reject, not how: the server's own 404 and a module's 405 both qualify.
        scorers.push(crate::eval::scenario::scenario(
            file!(),
            route_tag,
            host_url,
            lang,
            |s| {
                for (method, path) in [(reqwest::Method::PUT, "/items"), (reqwest::Method::GET, "/missing")] {
                    let (status, body) = s.http_method(method.clone(), path, "no")?;
                    anyhow::ensure!(
                        (400..500).contains(&status),
                        "{method} {path}: expected a rejection, got {status}: {body}"
                    );
                }
                Ok(())
            },
        ));
        scorers
    })
}
