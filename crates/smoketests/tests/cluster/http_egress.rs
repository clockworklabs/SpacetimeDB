use spacetimedb_smoketests::Smoketest;

#[test]
fn test_http_disallowed_ip_is_blocked() {
    let test = Smoketest::builder().precompiled_module("http-egress").build();

    let output = test.call_output("request_disallowed_ip", &["http://10.0.0.1:80/"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Expected request_disallowed_ip to succeed after observing blocked egress error.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}
