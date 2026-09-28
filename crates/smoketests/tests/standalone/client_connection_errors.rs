use socket2::SockRef;
use spacetimedb_smoketests::{require_local_server, Smoketest};
use std::io::Write;
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn test_http_reducer_call_cancel_still_deletes_st_client() {
    require_local_server!();

    let test = Smoketest::builder()
        .precompiled_module("client-connection-http-cancel")
        .build();

    let stream = send_http_reducer_call_and_hold_connection(&test, "slow");
    wait_for_log(
        &test,
        "http_cancel_repro: slow reducer started",
        Duration::from_secs(30),
    );
    abort_tcp_stream(stream);

    wait_for_log(
        &test,
        "http_cancel_repro: slow reducer finished",
        Duration::from_secs(30),
    );

    let (st_client_count, st_connection_credentials_count) = wait_for_client_rows_to_clear(&test);
    assert!(
        st_client_count <= 1,
        "Expected at most 1 st_client row (the SQL connection itself) after dropping an HTTP call post-connect, got {st_client_count}",
    );
    assert!(
        st_connection_credentials_count <= 1,
        "Expected at most 1 st_connection_credentials row (the SQL connection itself) after dropping an HTTP call post-connect, got {st_connection_credentials_count}",
    );
}

fn send_http_reducer_call_and_hold_connection(test: &Smoketest, reducer: &str) -> TcpStream {
    let host = test.server_host();
    let request = http_reducer_call_request(test, reducer);

    let mut stream = TcpStream::connect(host).expect("Failed to connect to local server");
    stream.set_nodelay(true).expect("Failed to set TCP_NODELAY");
    stream
        .write_all(request.as_bytes())
        .expect("Failed to write HTTP request");
    stream.flush().expect("Failed to flush HTTP request");
    stream
}

fn abort_tcp_stream(stream: TcpStream) {
    SockRef::from(&stream)
        .set_linger(Some(Duration::ZERO))
        .expect("Failed to set SO_LINGER=0");
    drop(stream);
}

fn http_reducer_call_request(test: &Smoketest, reducer: &str) -> String {
    let identity = test.database_identity.as_ref().expect("No database published");
    let host = test.server_host();
    let token = test.read_token().expect("Failed to read auth token");
    let path = format!("/v1/database/{identity}/call/{reducer}");

    // Keep the HTTP/1.1 connection alive so dropping the TCP stream is an
    // unexpected client disconnect while the reducer call is still active.
    format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Authorization: Bearer {token}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: 2\r\n\
         Connection: keep-alive\r\n\
         \r\n\
         []"
    )
}

fn wait_for_log(test: &Smoketest, needle: &str, timeout: Duration) -> Vec<String> {
    let start = Instant::now();
    loop {
        let logs = test.logs(200).unwrap();
        if logs.iter().any(|l| l.contains(needle)) {
            return logs;
        }
        assert!(
            start.elapsed() < timeout,
            "Timed out waiting for log containing {needle:?}: {:?}",
            logs
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_client_rows_to_clear(test: &Smoketest) -> (usize, usize) {
    let start = Instant::now();
    let mut last = (usize::MAX, usize::MAX);

    while start.elapsed() < Duration::from_secs(5) {
        last = (
            sql_count(test, "st_client"),
            sql_count(test, "st_connection_credentials"),
        );
        if last.0 <= 1 && last.1 <= 1 {
            return last;
        }
        thread::sleep(Duration::from_millis(50));
    }

    last
}

fn sql_count(test: &Smoketest, table: &str) -> usize {
    let output = test
        .sql(&format!("SELECT COUNT(*) AS count FROM {table}"))
        .unwrap_or_else(|err| panic!("Failed to count rows in {table}: {err:#}"));

    output
        .lines()
        .find_map(|line| line.trim().parse::<usize>().ok())
        .unwrap_or_else(|| panic!("Failed to parse COUNT(*) output for {table}: {output}"))
}
