use super::*;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};

pub(crate) const SOURCE: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const TARGET: &str = "0000000000000000000000000000000000000000000000000000000000000002";

pub(crate) fn token_body(token: &str, offset: u64) -> String {
    let expiry = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + offset;
    serde_json::json!({"token": token, "expires_unix_seconds": expiry}).to_string()
}

pub(crate) fn response(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ).into_bytes()
}

pub(crate) async fn read_request(socket: &mut (impl AsyncRead + Unpin)) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < MAX_HEADERS);
        bytes.push(socket.read_u8().await.unwrap());
    }
    let header = std::str::from_utf8(&bytes).unwrap();
    let length = header
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(length <= 4096);
    let start = bytes.len();
    bytes.resize(start + length, 0);
    socket.read_exact(&mut bytes[start..]).await.unwrap();
    String::from_utf8(bytes).unwrap()
}

// Every endpoint is created by this test, explicitly uses numeric loopback,
// and has a retained task that callers join. No saved server/client config.
pub(crate) async fn http_fixture(responses: Vec<Vec<u8>>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}/v1/credentials", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut requests = Vec::new();
            for response in responses {
                let (mut socket, peer) = listener.accept().await.unwrap();
                assert!(peer.ip().is_loopback());
                requests.push(read_request(&mut socket).await);
                socket.write_all(&response).await.unwrap();
                socket.shutdown().await.unwrap();
            }
            requests
        })
        .await
        .expect("owned credential fixture exceeded deadline")
    });
    (uri, task)
}

#[test]
fn discovery_rejects_nonlocal_ambiguous_and_credential_bearing_endpoints() {
    for endpoint in [
        "http://example.invalid/v1/credentials",
        "http://192.0.2.1/v1/credentials",
        "http://localhost/v1/credentials",
        "http://127.1/v1/credentials",
        "http://0x7f000001/v1/credentials",
        "http://127.0.0.1/wrong",
        "http://127.0.0.1/v1/credentials?token=never-log-this",
        "http://owner:never-log-this@127.0.0.1/v1/credentials",
        "https://127.0.0.1/v1/credentials",
        "unix://authority/socket",
        "unix:relative.sock",
        "unix:///run/../other.sock",
        "unix:///run/%2e%2e/other.sock",
        "unix:///run/credentials.sock#never-log-this",
        "spacetimedb-unavailable:///credentials",
    ] {
        let error = Container::new(SOURCE, "https://server.invalid", endpoint).unwrap_err();
        assert!(!format!("{error:?} {error}").contains("never-log-this"));
    }
    assert!(Container::new(SOURCE, "https://owner:secret@server.invalid", LOCAL_HTTP_URI).is_err());
    assert!(Container::new("self", "https://server.invalid", LOCAL_HTTP_URI).is_err());
    let config = Container::new(SOURCE, "wss://server.invalid/prefix", LOCAL_HTTP_URI).unwrap();
    assert_eq!(config.database_identity(), parse_identity(SOURCE).unwrap());
    assert_eq!(config.server_uri().to_string(), "https://server.invalid/prefix");
}

#[tokio::test]
async fn each_request_fetches_a_fresh_target_specific_token_without_sender_fields() {
    let (endpoint, server) = http_fixture(vec![
        response(200, &token_body("first-secret", 20)),
        response(200, &token_body("second-secret", 20)),
    ])
    .await;
    let config = Container::new(SOURCE, "https://server.invalid", &endpoint).unwrap();
    let target = parse_identity(TARGET).unwrap();
    let first = config.token_for(target).await.unwrap();
    let second = config.token_for(target).await.unwrap();
    assert_eq!(first.as_str(), "first-secret");
    assert_eq!(second.as_str(), "second-secret");
    assert_eq!(first.target(), target);
    assert!(first.remaining_lifetime() <= MAX_LIFETIME);
    assert!(!format!("{first:?}").contains("first-secret"));
    for request in server.await.unwrap() {
        assert!(request.starts_with("POST /v1/credentials HTTP/1.1\r\n"));
        assert_eq!(
            request.split_once("\r\n\r\n").unwrap().1,
            format!("{{\"target_database\":\"{TARGET}\"}}")
        );
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
    }
}

#[tokio::test]
async fn response_validation_rejects_expiry_fields_tokens_and_framing_without_secret_diagnostics() {
    let mut bad = vec![
        response(200, &token_body("", 20)),
        response(200, &token_body("never-log-this\n", 20)),
        response(200, &token_body(&"x".repeat(MAX_TOKEN + 1), 20)),
        response(200, &token_body("never-log-this", 0)),
        response(200, &token_body("never-log-this", 60)),
        response(200, "{\"token\":\"never-log-this\",\"expires_unix_seconds\":1,\"sender\":\"forged\"}"),
        response(200, "{\"token\":\"never-log-this\",\"token\":\"duplicate\",\"expires_unix_seconds\":1}"),
        response(200, "{\"token\":null,\"expires_unix_seconds\":null}"),
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1).into_bytes(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\nCache-Control: no-store\r\n\r\n0\r\n\r\n".to_vec(),
    ];
    let valid = String::from_utf8(response(200, &token_body("never-log-this", 20))).unwrap();
    bad.push(
        valid
            .replacen("\r\n", &format!("\r\nX-Padding: {}\r\n", "x".repeat(MAX_HEADERS)), 1)
            .into_bytes(),
    );
    bad.push(valid.replace("Cache-Control: no-store\r\n", "").into_bytes());
    bad.push(
        valid
            .replace("Content-Type: application/json", "Content-Type: text/plain")
            .into_bytes(),
    );
    for bytes in bad {
        let (endpoint, server) = http_fixture(vec![bytes]).await;
        let config = Container::new(SOURCE, "https://server.invalid", &endpoint).unwrap();
        let error = config.token_for(parse_identity(TARGET).unwrap()).await.unwrap_err();
        assert!(matches!(
            error,
            ContainerCredentialError::InvalidResponse | ContainerCredentialError::Transport
        ));
        assert!(!format!("{error:?} {error}").contains("never-log-this"));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn broker_denial_unavailability_and_redirects_never_fall_back() {
    for (status, expected) in [
        (401, ContainerCredentialError::Denied),
        (403, ContainerCredentialError::Denied),
        (503, ContainerCredentialError::Unavailable),
        (302, ContainerCredentialError::InvalidResponse),
    ] {
        let bytes = String::from_utf8(response(status, "never-log-this"))
            .unwrap()
            .replacen("\r\n", "\r\nLocation: https://must-not-contact.invalid/\r\n", 1)
            .into_bytes();
        let (endpoint, server) = http_fixture(vec![bytes]).await;
        let config = Container::new(SOURCE, "https://server.invalid", &endpoint).unwrap();
        assert_eq!(
            config.token_for(parse_identity(TARGET).unwrap()).await.unwrap_err(),
            expected
        );
        assert_eq!(server.await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn cancelling_a_pending_request_closes_its_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/credentials", listener.local_addr().unwrap());
    let config = Container::new(SOURCE, "https://server.invalid", &endpoint).unwrap();
    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        ready_tx.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), socket.read_u8())
                .await
                .unwrap()
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    });
    let mut request = Box::pin(config.token_for(parse_identity(TARGET).unwrap()));
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::select! {
            _ = &mut request => panic!("stalled broker unexpectedly answered"),
            ready = ready_rx => ready.unwrap(),
        }
    })
    .await
    .unwrap();
    drop(request);
    server.await.unwrap();
}

#[tokio::test]
async fn self_alias_stays_local_and_name_resolution_pins_the_returned_identity() {
    let (broker, broker_task) = http_fixture(vec![
        response(200, &token_body("self-token", 20)),
        response(200, &token_body("other-token", 20)),
    ])
    .await;
    let config = Container::new(SOURCE, "https://must-not-contact.invalid", &broker).unwrap();
    let own = config.prepare_connection(None, Some("self")).await.unwrap();
    assert_eq!(own.target, SOURCE);
    let (resolver, resolver_task) = http_fixture(vec![response(200, TARGET)]).await;
    let server: Uri = resolver.trim_end_matches("v1/credentials").parse().unwrap();
    let other = config.prepare_connection(Some(&server), Some("tasks")).await.unwrap();
    assert_eq!(other.target, TARGET);
    assert_eq!(other.credential.target(), parse_identity(TARGET).unwrap());
    let requests = broker_task.await.unwrap();
    assert!(requests[0].ends_with(&format!("{{\"target_database\":\"{SOURCE}\"}}")));
    assert!(requests[1].ends_with(&format!("{{\"target_database\":\"{TARGET}\"}}")));
    assert!(resolver_task.await.unwrap()[0].starts_with("GET /v1/database/tasks/identity HTTP/1.1\r\n"));
}

#[cfg(unix)]
#[tokio::test]
async fn unix_socket_uses_the_same_bounded_http_protocol_without_tcp_fallback() {
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tokio::net::UnixListener;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    // Use an explicitly short base because ambient TMPDIR can exceed sockaddr_un
    // limits on macOS. Atomic creation and private permissions retain ownership.
    let directory = PathBuf::from("/tmp").join(format!(
        "sdk-cred-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let directory = Directory(directory);
    let socket = directory.0.join("broker.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let config = Container::new(
        SOURCE,
        "https://server.invalid",
        &format!("unix://{}", socket.display()),
    )
    .unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_request(&mut socket).await;
        socket
            .write_all(&response(200, &token_body("unix-secret", 20)))
            .await
            .unwrap();
        socket.shutdown().await.unwrap();
        request
    });
    assert_eq!(
        config
            .token_for(parse_identity(TARGET).unwrap())
            .await
            .unwrap()
            .as_str(),
        "unix-secret"
    );
    let request = tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    assert!(request.to_ascii_lowercase().contains("host: 127.0.0.1:18081\r\n"));
    // The listener is gone but its pathname is retained: failure must not fall
    // back to the HTTP authority used to format requests on this transport.
    assert!(config.token_for(parse_identity(TARGET).unwrap()).await.is_err());
}
