use spacetimedb_smoketests::{require_local_server, Smoketest};

const MODULE_CODE_HTTP_DISALLOWED_IP: &str = r#"
use spacetimedb::ProcedureContext;

#[spacetimedb::procedure]
pub fn request_disallowed_ip(ctx: &mut ProcedureContext) -> Result<(), String> {
    match ctx.http.get("http://127.0.0.1:80/") {
        Ok(_) => Err("request unexpectedly succeeded".to_owned()),
        Err(err) => {
            let message = err.to_string();
            if message.contains("refusing to connect to private or special-purpose addresses") {
                Ok(())
            } else {
                Err(format!("unexpected error from http request: {message}"))
            }
        }
    }
}
"#;

#[test]
fn test_http_disallowed_ip_is_blocked() {
    require_local_server!();

    let test = Smoketest::builder().module_code(MODULE_CODE_HTTP_DISALLOWED_IP).build();

    let output = test.call_output("request_disallowed_ip", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Expected request_disallowed_ip to succeed after observing blocked egress error.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}
