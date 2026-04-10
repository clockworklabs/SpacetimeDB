use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use spacetimedb_smoketests::Smoketest;

/// Spawn a test HTTP server that responds with chunked data and custom headers.
/// Accepts up to `max_connections` requests then exits.
fn spawn_chunked_server(max_connections: usize) -> (u16, JoinHandle<std::io::Result<()>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("failed to bind test server");
    listener
        .set_nonblocking(true)
        .expect("failed to set test server nonblocking mode");
    let port = listener
        .local_addr()
        .expect("failed to read test server address")
        .port();

    let handle = std::thread::spawn(move || -> std::io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        for _ in 0..max_connections {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(pair) => break pair,
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            // All expected connections may not arrive (e.g. if a test fails early).
                            return Ok(());
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => return Err(err),
                }
            };

            // Drain the request.
            let mut buf = [0u8; 4096];
            stream.set_read_timeout(Some(Duration::from_millis(200))).ok();
            let _ = stream.read(&mut buf);

            // Respond with chunked encoding and custom headers.
            stream.write_all(
                b"HTTP/1.1 200 OK\r\n\
                  Content-Type: text/event-stream\r\n\
                  X-Test-Header: hello-world\r\n\
                  Transfer-Encoding: chunked\r\n\
                  Connection: close\r\n\r\n",
            )?;

            // Send three chunks.
            for chunk in &[b"AAA" as &[u8], b"BBB", b"CCC"] {
                write!(stream, "{:x}\r\n", chunk.len())?;
                stream.write_all(chunk)?;
                stream.write_all(b"\r\n")?;
            }
            // Terminating chunk.
            stream.write_all(b"0\r\n\r\n")?;
            stream.flush()?;
        }
        Ok(())
    });

    (port, handle)
}

fn ts_module_code(port: u16) -> String {
    format!(
        r#"import {{ schema, t, table }} from "spacetimedb/server";

const spacetimedb = schema({{}});
export default spacetimedb;

// --- Test 1: basic streaming read ---
export const stream_read = spacetimedb.procedure(
  {{}},
  t.string(),
  (ctx) => {{
    const resp = ctx.http.fetchStreaming("http://127.0.0.1:{port}/stream");
    const decoder = new TextDecoder();
    let body = "";
    for (const chunk of resp) {{
      body += decoder.decode(chunk);
    }}
    return body;
  }}
);

// --- Test 2: stream_next inside a transaction must throw ---
export const stream_next_blocked_in_tx = spacetimedb.procedure(
  {{}},
  t.string(),
  (ctx) => {{
    // Open the stream *outside* the transaction — this must succeed.
    const resp = ctx.http.fetchStreaming("http://127.0.0.1:{port}/stream");
    try {{
      ctx.withTx(_tx => {{
        // Iterating calls procedure_http_stream_next which should throw
        // WouldBlockTransaction because a mutable tx is open.
        for (const _chunk of resp) {{
          return "ERROR: stream.next() inside tx should have thrown";
        }}
      }});
      return "ERROR: withTx should have thrown";
    }} catch (e: any) {{
      // We expect a WouldBlockTransaction error.
      return "blocked";
    }} finally {{
      resp[Symbol.dispose]();
    }}
  }}
);

// --- Test 3: streaming response headers are preserved ---
export const stream_headers = spacetimedb.procedure(
  {{}},
  t.string(),
  (ctx) => {{
    const resp = ctx.http.fetchStreaming("http://127.0.0.1:{port}/stream");
    try {{
      const ct = resp.headers.get("content-type") ?? "MISSING";
      const xh = resp.headers.get("x-test-header") ?? "MISSING";
      return ct + "|" + xh;
    }} finally {{
      resp[Symbol.dispose]();
    }}
  }}
);
"#
    )
}

/// Test that streaming HTTP responses can be read chunk by chunk,
/// that iterating a stream inside a transaction is blocked,
/// and that response headers are preserved.
///
/// Requires the server to be built with `allow_loopback_http_for_tests`.
#[test]
fn test_http_streaming() {
    spacetimedb_smoketests::require_pnpm!();

    // 3 connections: one per procedure call.
    let (port, server) = spawn_chunked_server(3);

    let mut test = Smoketest::builder().autopublish(false).build();
    test.publish_typescript_module_source("http-streaming", "http-streaming", &ts_module_code(port))
        .unwrap();

    // Test 1: basic streaming read — all chunks concatenated.
    let output = test.call_output("stream_read", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stream_read failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("AAABBBCCC"),
        "expected concatenated chunks AAABBBCCC in output, got:\n{stdout}"
    );

    // Test 2: stream_next inside a transaction must throw.
    let output = test.call_output("stream_next_blocked_in_tx", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stream_next_blocked_in_tx failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("blocked"),
        "expected procedure to catch WouldBlockTransaction, got:\n{stdout}"
    );

    // Test 3: response headers are preserved.
    let output = test.call_output("stream_headers", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stream_headers failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("text/event-stream") && stdout.contains("hello-world"),
        "expected response headers in output, got:\n{stdout}"
    );

    server
        .join()
        .expect("test server thread panicked")
        .expect("test server failed");
}
