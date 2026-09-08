use super::*;
use axum::{
    body::{Body, Bytes},
    extract::Path,
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use spacetimedb_lib::container::{endpoints::ContainerEndpoint, PortProtocol};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn response() -> ContainerEndpoints {
    ContainerEndpoints {
        database_identity: Identity::ZERO,
        endpoints: vec![ContainerEndpoint {
            name: "http".into(),
            protocol: PortProtocol::Http,
            url: "https://aaaqeayeaudaocajbifqydiob4.container.example.net/".into(),
        }],
    }
}

struct Server {
    origin: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Server {
    async fn start(router: Router) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let origin = format!("http://{}", listener.local_addr()?);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        Ok(Self {
            origin,
            stop: Some(stop),
            task,
        })
    }

    async fn shutdown(mut self) -> Result<()> {
        self.stop.take().unwrap().send(()).ok();
        tokio::time::timeout(Duration::from_secs(5), &mut self.task).await???;
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn discovery_is_anonymous_and_encodes_the_entire_database_path_segment() -> Result<()> {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let server = Server::start(Router::new().route(
        "/v1/database/:database/container/endpoints",
        get(move |Path(database): Path<String>, headers: HeaderMap| {
            let count = count.clone();
            async move {
                assert_eq!(database, "project/child?query=value#fragment");
                assert!(!headers.contains_key(header::AUTHORIZATION));
                assert!(!headers.contains_key(header::COOKIE));
                count.fetch_add(1, Ordering::SeqCst);
                Json(response())
            }
        }),
    ))
    .await?;
    for invalid in [".", ".."] {
        assert!(fetch(&server.origin, invalid).await.is_err());
    }
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    let discovered = fetch(&server.origin, "project/child?query=value#fragment").await?;
    assert_eq!(select(&discovered, None)?, response().endpoints[0].url);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    server.shutdown().await
}

#[test]
fn port_selection_never_picks_an_arbitrary_endpoint() -> Result<()> {
    let mut endpoints = response();
    assert_eq!(select(&endpoints, Some("http"))?, endpoints.endpoints[0].url);
    assert!(select(&endpoints, Some("absent")).is_err());
    let mut second = endpoints.endpoints[0].clone();
    second.name = "metrics".into();
    second.url = "https://bbbbbbbbbbbbbbbbbbbbbbbbaa.container.example.net/".into();
    endpoints.endpoints.push(second);
    assert!(select(&endpoints, None)
        .unwrap_err()
        .to_string()
        .contains("--port: http, metrics"));
    assert_eq!(select(&endpoints, Some("metrics"))?, endpoints.endpoints[1].url);
    endpoints.endpoints.clear();
    assert!(select(&endpoints, None)
        .unwrap_err()
        .to_string()
        .contains("no declared public HTTP ports"));
    Ok(())
}

#[test]
fn endpoint_output_rejects_terminal_controls_credentials_and_ambiguous_names() {
    let mut endpoints = response();
    for address in [
        "javascript:alert(1)",
        "http://application.example/",
        "https://user:secret@application.example/",
        "https://127.0.0.1/",
        "https://application.example/path",
        "https://application.example/?secret=value",
        "https://application.example/#fragment",
        "https://application.example:8443/",
        "https://application.example/\n",
        "\u{1b}[2Jhttps://application.example/",
    ] {
        endpoints.endpoints[0].url = address.into();
        assert!(validate(&endpoints).is_err());
    }
    endpoints = response();
    for name in ["", "HTTP", "1http", "http\n", "http/metrics"] {
        endpoints.endpoints[0].name = name.into();
        assert!(validate(&endpoints).is_err());
    }
    endpoints = response();
    endpoints.endpoints.push(endpoints.endpoints[0].clone());
    assert!(validate(&endpoints).is_err());
}

#[tokio::test]
async fn discovery_bounds_streams_rejects_redirects_and_preserves_pending_errors() -> Result<()> {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let server = Server::start(Router::new().route(
        "/v1/database/:database/container/endpoints",
        get(move |Path(database): Path<String>| {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                match database.as_str() {
                    "pending" => (StatusCode::SERVICE_UNAVAILABLE, "private diagnostic").into_response(),
                    "missing" => StatusCode::NOT_FOUND.into_response(),
                    "redirect" => (StatusCode::FOUND, [(header::LOCATION, "/unexpected")]).into_response(),
                    "invalid" => Json(serde_json::json!({"private diagnostic": "do not print"})).into_response(),
                    "oversize" => Response::new(Body::from_stream(futures::stream::iter([
                        Ok::<_, std::io::Error>(Bytes::from(vec![b' '; MAX_RESPONSE_BYTES])),
                        Ok(Bytes::from_static(b"x")),
                    ]))),
                    _ => Json(response()).into_response(),
                }
            }
        }),
    ))
    .await?;
    for (database, expected) in [
        ("pending", "not available yet"),
        ("missing", "not found"),
        ("redirect", "HTTP 302"),
        ("invalid", "invalid container endpoint discovery response"),
        ("oversize", "size limit"),
    ] {
        let error = format!("{:#}", fetch(&server.origin, database).await.unwrap_err());
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("private diagnostic"));
    }
    let other_identity = Identity::from_u256(1_u64.into()).to_string();
    assert!(fetch(&server.origin, &other_identity)
        .await
        .unwrap_err()
        .to_string()
        .contains("another database Identity"));
    assert_eq!(requests.load(Ordering::SeqCst), 6);
    server.shutdown().await
}

#[test]
fn url_command_requires_database_and_parses_explicit_server_and_port() {
    assert!(cli().try_get_matches_from(["url"]).is_err());
    let args = cli()
        .try_get_matches_from(["url", "demo", "--server", "http://127.0.0.1:3000", "--port", "http"])
        .unwrap();
    assert_eq!(args.get_one::<String>("database").unwrap(), "demo");
    assert_eq!(args.get_one::<String>("server").unwrap(), "http://127.0.0.1:3000");
    assert_eq!(args.get_one::<String>("port").unwrap(), "http");
}
