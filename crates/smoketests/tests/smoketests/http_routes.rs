use spacetimedb_smoketests::Smoketest;

#[test]
fn test_http_route_get() {
    let module_code = r#"
use spacetimedb::{procedure, ProcedureContext, http::{Request, Response, Body}};

#[procedure(route = get("/hello"))]
fn hello(_ctx: &mut ProcedureContext, _request: Request) -> Response<Body> {
    Response::builder()
        .status(200)
        .body(Body::from("HELLO WORLD"))
        .unwrap()
}
"#;

    let test = Smoketest::builder().module_code(module_code).build();
    let identity = test.database_identity.as_ref().expect("No database published");

    let response = test
        .api_call("GET", &format!("/v1/database/{}/route/hello", identity))
        .expect("HTTP route request failed");

    assert_eq!(response.status_code, 200);
    assert_eq!(String::from_utf8_lossy(&response.body), "HELLO WORLD");
}
