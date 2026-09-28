use spacetimedb_smoketests::{allow_dotnet, random_string, Smoketest};

const NO_SUCH_ROUTE_BODY: &str = "Database has not registered a handler for this route";

fn rust_http_test(module: &str) -> (Smoketest, String) {
    let test = Smoketest::builder().precompiled_module(module).build();
    let identity = test
        .database_identity
        .as_ref()
        .expect("database identity missing")
        .clone();
    (test, identity)
}

fn cpp_http_test(name: &str) -> (Smoketest, String) {
    let mut test = Smoketest::builder().precompiled_module(name).autopublish(false).build();
    let identity = test.publish().name(name).run().unwrap();
    (test, identity)
}

fn typescript_http_test(name: &str) -> (Smoketest, String) {
    let mut test = Smoketest::builder().precompiled_module(name).autopublish(false).build();
    let database_name = format!("{name}-{}", random_string());
    let identity = test.publish().name(&database_name).run().unwrap();
    (test, identity)
}

fn csharp_http_test(name: &str) -> (Smoketest, String) {
    let mut test = Smoketest::builder().precompiled_module(name).autopublish(false).build();
    let identity = test.publish().name(name).run().unwrap();
    (test, identity)
}

fn route_base(server_url: &str, identity: &str) -> String {
    format!("{server_url}/v1/database/{identity}/route")
}

fn assert_http_routes_end_to_end(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(format!("{base}/get")).send().expect("get failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("get body"), "ok");

    let resp = client
        .post(format!("{base}/post"))
        .body("payload")
        .send()
        .expect("post failed");
    assert!(resp.status().is_success());

    let resp = client.get(format!("{base}/count")).send().expect("count failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("count body"), "1");

    let resp = client.put(format!("{base}/any")).send().expect("any failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("any body"), "any");

    let resp = client
        .get(format!("{base}/header"))
        .header("x-echo", "hello")
        .send()
        .expect("header echo failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("header body"), "hello");

    let resp = client
        .get(format!("{base}/set-header"))
        .send()
        .expect("set-header failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.headers().get("x-response").and_then(|value| value.to_str().ok()),
        Some("set")
    );

    let resp = client.get(format!("{base}/body")).send().expect("body failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("body text"), "non-empty");

    let resp = client.get(format!("{base}/teapot")).send().expect("teapot failed");
    assert_eq!(resp.status().as_u16(), 418);

    let resp = client
        .get(format!("{base}/missing"))
        .send()
        .expect("missing route failed");
    assert_eq!(resp.status().as_u16(), 404);
    assert_eq!(resp.text().expect("missing route body"), NO_SUCH_ROUTE_BODY);

    let resp = client
        .get(format!("{server_url}/v1/database/{identity}/schema?version=10"))
        .header("authorization", "Bearer not-a-jwt")
        .send()
        .expect("schema request failed");
    assert!(resp.status().is_client_error());

    let resp = client
        .get(format!("{base}/get"))
        .header("authorization", "Bearer not-a-jwt")
        .send()
        .expect("route request failed");
    assert!(resp.status().is_success());
}

fn assert_http_routes_pr_example_round_trip(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
    let client = reqwest::blocking::Client::new();
    let payload = b"hello from the PR example".to_vec();

    let resp = client
        .post(format!("{base}/insert"))
        .body(payload.clone())
        .send()
        .expect("insert failed");
    assert!(resp.status().is_success());
    let inserted_id = resp.text().expect("insert id body");

    let resp = client
        .get(format!("{base}/retrieve?id={inserted_id}"))
        .send()
        .expect("retrieve existing failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.bytes().expect("retrieve existing body").as_ref(),
        payload.as_slice()
    );

    let resp = client
        .get(format!("{base}/retrieve?id=999999999"))
        .send()
        .expect("retrieve missing failed");
    assert_eq!(resp.status().as_u16(), 404);

    let resp = client
        .get(format!("{base}/retrieve?id=not-a-u64"))
        .send()
        .expect("retrieve invalid failed");
    assert!(resp.status().is_server_error());
}

fn assert_http_routes_are_strict_for_non_root_paths(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(format!("{base}/foo")).send().expect("foo failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("foo body"), "foo");

    let resp = client.get(format!("{base}/foo/")).send().expect("foo slash failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("foo slash body"), "foo-slash");

    let resp = client.get(format!("{base}//")).send().expect("double slash failed");
    assert_eq!(resp.status().as_u16(), 404);
    assert_eq!(resp.text().expect("double slash body"), NO_SUCH_ROUTE_BODY);

    let resp = client
        .get(format!("{base}//foo"))
        .send()
        .expect("double slash foo failed");
    assert_eq!(resp.status().as_u16(), 404);
    assert_eq!(resp.text().expect("double slash foo body"), NO_SUCH_ROUTE_BODY);
}

fn assert_http_routes_are_strict_for_root_paths(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(base.clone()).send().expect("empty root failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("empty root body"), "empty");

    let resp = client.get(format!("{base}/")).send().expect("slash root failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("slash root body"), "slash");
}

fn assert_http_handler_observes_full_external_uri(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
    let url = format!("{base}/echo-uri?alpha=beta");
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("echo-uri failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("echo-uri body"), url);
}

fn assert_handle_request_body(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client
        .post(format!("{base}/reverse-bytes"))
        .body(vec![0xFF, 0x00, 0xFE, 0x7F])
        .send()
        .expect("reverse-bytes invalid utf-8 failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.bytes().expect("reverse-bytes invalid utf-8 body").as_ref(),
        [0x7F, 0xFE, 0x00, 0xFF]
    );

    let resp = client
        .post(format!("{base}/reverse-bytes"))
        .body("abcba")
        .send()
        .expect("reverse-bytes palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.bytes().expect("reverse-bytes palindrome body").as_ref(), b"abcba");

    let resp = client
        .post(format!("{base}/reverse-bytes"))
        .body("stressed")
        .send()
        .expect("reverse-bytes non-palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.bytes().expect("reverse-bytes non-palindrome body").as_ref(),
        b"desserts"
    );

    let resp = client
        .post(format!("{base}/reverse-words"))
        .body(vec![0x66, 0x6F, 0x80, 0x6F])
        .send()
        .expect("reverse-words invalid utf-8 failed");
    assert_eq!(resp.status().as_u16(), 400);
    assert_eq!(
        resp.text().expect("reverse-words invalid utf-8 body"),
        "request body must be valid UTF-8"
    );

    let resp = client
        .post(format!("{base}/reverse-words"))
        .body("step on no pets")
        .send()
        .expect("reverse-words palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("reverse-words palindrome body"), "pets no on step");

    let resp = client
        .post(format!("{base}/reverse-words"))
        .body("red green blue")
        .send()
        .expect("reverse-words non-palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.text().expect("reverse-words non-palindrome body"),
        "blue green red"
    );
}

#[test]
fn http_routes_end_to_end() {
    let (test, identity) = rust_http_test("http-routes");
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn http_routes_pr_example_round_trip() {
    let (test, identity) = rust_http_test("http-routes-example");
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn http_routes_are_strict_for_non_root_paths() {
    let (test, identity) = rust_http_test("http-routes-strict-non-root");
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn http_routes_are_strict_for_root_paths() {
    let (test, identity) = rust_http_test("http-routes-strict-root");
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn http_handler_observes_full_external_uri() {
    let (test, identity) = rust_http_test("http-routes-full-uri");
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn handle_request_body() {
    let (test, identity) = rust_http_test("http-routes-request-body");
    assert_handle_request_body(&test.server_url, &identity);
}

#[test]
fn cpp_http_routes_end_to_end() {
    let (test, identity) = cpp_http_test("http-routes-cpp-basic");
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_end_to_end() {
    let (test, identity) = typescript_http_test("http-routes-typescript-basic");
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_end_to_end() {
    if !allow_dotnet() {
        return;
    }
    let (test, identity) = csharp_http_test("http-routes-csharp-basic");
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn cpp_http_routes_pr_example_round_trip() {
    let (test, identity) = cpp_http_test("http-routes-cpp-example");
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_pr_example_round_trip() {
    let (test, identity) = typescript_http_test("http-routes-typescript-example");
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_pr_example_round_trip() {
    if !allow_dotnet() {
        return;
    }
    let (test, identity) = csharp_http_test("http-routes-csharp-example");
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn cpp_http_routes_are_strict_for_non_root_paths() {
    let (test, identity) = cpp_http_test("http-routes-cpp-strict-non-root");
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_are_strict_for_non_root_paths() {
    let (test, identity) = typescript_http_test("http-routes-typescript-strict-non-root");
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_are_strict_for_non_root_paths() {
    if !allow_dotnet() {
        return;
    }
    let (test, identity) = csharp_http_test("http-routes-csharp-strict-non-root");
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn cpp_http_routes_are_strict_for_root_paths() {
    let (test, identity) = cpp_http_test("http-routes-cpp-strict-root");
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_are_strict_for_root_paths() {
    let (test, identity) = typescript_http_test("http-routes-typescript-strict-root");
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_are_strict_for_root_paths() {
    if !allow_dotnet() {
        return;
    }
    let (test, identity) = csharp_http_test("http-routes-csharp-strict-root");
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn cpp_http_handler_observes_full_external_uri() {
    let (test, identity) = cpp_http_test("http-routes-cpp-full-uri");
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn typescript_http_handler_observes_full_external_uri() {
    let (test, identity) = typescript_http_test("http-routes-typescript-full-uri");
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn csharp_http_handler_observes_full_external_uri() {
    if !allow_dotnet() {
        return;
    }
    let (test, identity) = csharp_http_test("http-routes-csharp-full-uri");
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn cpp_handle_request_body() {
    let (test, identity) = cpp_http_test("http-routes-cpp-request-body");
    assert_handle_request_body(&test.server_url, &identity);
}

#[test]
fn typescript_handle_request_body() {
    let (test, identity) = typescript_http_test("http-routes-typescript-request-body");
    assert_handle_request_body(&test.server_url, &identity);
}

#[test]
fn csharp_handle_request_body() {
    if !allow_dotnet() {
        return;
    }
    let (test, identity) = csharp_http_test("http-routes-csharp-request-body");
    assert_handle_request_body(&test.server_url, &identity);
}

/// Validates the Rust example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn http_handlers_tutorial_say_hello_route_works() {
    let test = Smoketest::builder()
        .precompiled_module("http-handlers-tutorial")
        .build();
    let identity = test.database_identity.as_ref().expect("database identity missing");

    let url = format!("{}/v1/database/{}/route/say-hello", test.server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

/// Validates the C++ example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn cpp_http_handlers_tutorial_say_hello_route_works() {
    let (test, identity) = cpp_http_test("http-handlers-docs-cpp");

    let url = format!("{}/v1/database/{identity}/route/say-hello", test.server_url);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

/// Validates the TypeScript example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn typescript_http_handlers_tutorial_say_hello_route_works() {
    let (test, identity) = typescript_http_test("http-handlers-docs-typescript");

    let url = format!("{}/v1/database/{identity}/route/say-hello", test.server_url);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

/// Validates the C# example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn csharp_http_handlers_tutorial_say_hello_route_works() {
    if !allow_dotnet() {
        return;
    }
    let (test, identity) = csharp_http_test("http-handlers-docs-csharp");

    let url = format!("{}/v1/database/{}/route/say-hello", test.server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}
