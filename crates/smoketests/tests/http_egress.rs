use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use spacetimedb_smoketests::Smoketest;

fn module_code_http_disallowed_ip(addr: &str, port: u16) -> String {
    format!(
        r#"
use spacetimedb::ProcedureContext;

#[spacetimedb::procedure]
pub fn request_redirect_to_disallowed_ip(ctx: &mut ProcedureContext) -> Result<(), String> {{
    match ctx.http.get("http://{addr}:{port}/") {{
        Ok(_) => Err("request unexpectedly succeeded".to_owned()),
        Err(err) => {{
            let message = err.to_string();
            if message.contains("refusing to connect to private or special-purpose addresses") {{
                Ok(())
            }} else {{
                Err(format!("unexpected error from http request: {{message}}"))
            }}
        }}
    }}
}}
"#
    )
}

fn spawn_redirect_server(location: &str) -> (u16, JoinHandle<std::io::Result<()>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("failed to bind test redirect server");
    let port = listener
        .local_addr()
        .expect("failed to read test redirect server address")
        .port();
    let location = location.to_owned();
    let handle = std::thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf)?;
        let response =
            format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        stream.write_all(response.as_bytes())?;
        stream.flush()?;
        Ok(())
    });
    (port, handle)
}

#[test]
fn test_http_disallowed_ip_is_blocked() {
    let module_code = module_code_http_disallowed_ip("10.0.0.1", 80);
    let test = Smoketest::builder().module_code(&module_code).build();

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

#[test]
fn test_http_redirect_to_disallowed_ip_is_blocked() {
    let (port, redirect_server) = spawn_redirect_server("http://10.0.0.1:80/");
    let module_code = module_code_http_disallowed_ip("localhost", port);
    let test = Smoketest::builder().module_code(&module_code).build();

    let output = test.call_output("request_redirect_to_disallowed_ip", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Expected request_redirect_to_disallowed_ip to succeed after observing blocked egress error.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    redirect_server
        .join()
        .expect("redirect test server thread panicked")
        .expect("redirect test server failed");
}
