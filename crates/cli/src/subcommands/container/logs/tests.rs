use super::*;
use axum::{
    body::{Body, Bytes},
    extract::{Path, Query},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn page() -> ContainerLogPage {
    ContainerLogPage {
        database_identity: Identity::ONE,
        generation: 7,
        deployment_revision: Hash::from_byte_array([3; 32]),
        publication_operation: Uuid::from_u128(1),
        publication_epoch: 2,
        capture_id: Uuid::from_u128(3),
        records: vec![
            LogRecord {
                sequence: 1,
                event: LogEvent::Data {
                    timestamp_micros: 1,
                    stream: LogStream::Stdout,
                    bytes: vec![0, 255, b'a'],
                },
            },
            LogRecord {
                sequence: 2,
                event: LogEvent::Data {
                    timestamp_micros: 2,
                    stream: LogStream::Stderr,
                    bytes: vec![254, b'\n'],
                },
            },
        ],
        next_cursor: "cursor_2".into(),
        oldest_retained_sequence: 1,
        retention_gap: false,
        has_more: false,
        end: None,
        loss: None,
    }
}

#[test]
fn output_preserves_bytes_streams_and_structured_metadata() -> Result<()> {
    let page = page();
    let (mut out, mut err) = (vec![], vec![]);
    write_page(&page, false, &mut out, &mut err, &mut None)?;
    assert_eq!(out, [0, 255, b'a']);
    assert_eq!(err, [254, b'\n']);
    out.clear();
    err.clear();
    write_page(&page, true, &mut out, &mut err, &mut None)?;
    assert!(err.is_empty());
    let decoded: ContainerLogPage = serde_json::from_slice(&out)?;
    assert!(decoded == page);
    assert_eq!(out.last(), Some(&b'\n'));
    Ok(())
}

#[test]
fn continuation_pins_capture_and_rejects_replays_or_silent_gaps() -> Result<()> {
    let mut selection = Selection::new(Some(Identity::ONE), &ContainerLogQuery::default());
    selection.accept(&page())?;
    let mut next = page();
    next.records.clear();
    selection.accept(&next)?; // Empty heartbeat can retain the same cursor.
    for changed in 0..7 {
        let mut different = next.clone();
        match changed {
            0 => different.database_identity = Identity::ZERO,
            1 => different.generation += 1,
            2 => different.capture_id = Uuid::from_u128(4),
            3 => different.deployment_revision = Hash::from_byte_array([4; 32]),
            4 => different.publication_epoch += 1,
            5 => different.publication_operation = Uuid::from_u128(5),
            _ => different.records = page().records,
        }
        assert!(selection.accept(&different).is_err());
    }
    next.records.push(LogRecord {
        sequence: 4,
        event: LogEvent::Data {
            timestamp_micros: 3,
            stream: LogStream::Stdout,
            bytes: b"last".to_vec(),
        },
    });
    next.next_cursor = "cursor_4".into();
    assert!(selection.accept(&next).is_err());
    next.retention_gap = true;
    next.oldest_retained_sequence = 4;
    next.end = Some(LogEnd::Eof);
    next.loss = Some(LogLoss::DrainTimeout);
    selection.accept(&next)?;
    next.records.clear();
    next.loss = None;
    assert!(selection.accept(&next).is_err());
    Ok(())
}

#[test]
fn incomplete_capture_and_retention_are_visible_without_repeating_loss() -> Result<()> {
    let mut page = page();
    page.records.clear();
    page.retention_gap = true;
    page.loss = Some(LogLoss::DrainTimeout);
    page.end = Some(LogEnd::Eof);
    let (mut out, mut err, mut reported) = (vec![], vec![], None);
    write_page(&page, false, &mut out, &mut err, &mut reported)?;
    let text = String::from_utf8(err.clone())?;
    assert!(text.contains("removed by retention"));
    assert!(text.contains("incomplete capture, DrainTimeout"));
    assert!(out.is_empty());
    page.retention_gap = false;
    err.clear();
    write_page(&page, false, &mut out, &mut err, &mut reported)?;
    assert!(err.is_empty());
    Ok(())
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
async fn requests_preserve_selection_and_bound_untrusted_responses() -> Result<()> {
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    let server = Server::start(Router::new().route(
        "/v1/database/:database/container/logs",
        get(
            move |Path(database): Path<String>, Query(query): Query<ContainerLogQuery>, headers: HeaderMap| {
                let requests = requests.clone();
                async move {
                    requests.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(headers.get(header::AUTHORIZATION).unwrap(), "Bearer disposable-test");
                    assert!(!headers.contains_key(header::COOKIE));
                    assert_eq!(query.generation, Some(7));
                    assert_eq!(query.cursor.as_deref(), Some("cursor_0"));
                    assert!(query.follow);
                    match database.as_str() {
                        "oversize" => Response::new(Body::from_stream(futures::stream::iter([
                            Ok::<_, std::io::Error>(Bytes::from(vec![b' '; MAX_LOG_PAGE_BYTES])),
                            Ok(Bytes::from_static(b"x")),
                        ]))),
                        "redirect" => (StatusCode::FOUND, [(header::LOCATION, "/unexpected")]).into_response(),
                        "denied" => (
                            StatusCode::FORBIDDEN,
                            Json(ContainerApiError {
                                error: ContainerErrorCode::AccessDenied,
                            }),
                        )
                            .into_response(),
                        "invalid" => Json(serde_json::json!({"private diagnostic": "do not print"})).into_response(),
                        "project/child?query#fragment" => Json(page()).into_response(),
                        _ => StatusCode::NOT_FOUND.into_response(),
                    }
                }
            },
        ),
    ))
    .await?;
    let result = async {
        let client = ContainerClient::new(reqwest::Url::parse(&server.origin)?, "Bearer disposable-test".parse()?)?;
        let query = ContainerLogQuery {
            generation: Some(7),
            cursor: Some("cursor_0".into()),
            follow: true,
        };
        let actual = fetch(&client, "project/child?query#fragment", &query).await?;
        ensure!(actual == page(), "received different page");
        for (database, expected) in [
            ("oversize", "size limit"),
            ("redirect", "HTTP 302"),
            ("denied", "does not permit"),
            ("invalid", "invalid container log response"),
        ] {
            let error = match fetch(&client, database, &query).await {
                Ok(_) => bail!("expected rejection"),
                Err(error) => error.to_string(),
            };
            ensure!(
                error.contains(expected) && !error.contains("private diagnostic"),
                "wrong fixed error"
            );
        }
        ensure!(count.load(Ordering::SeqCst) == 5, "unexpected redirected request");
        Ok(())
    }
    .await;
    let shutdown = server.shutdown().await;
    result.and(shutdown)
}

#[test]
fn command_requires_generation_for_cursor_and_parses_explicit_server() {
    assert!(cli()
        .try_get_matches_from(["logs", "demo", "--cursor", "resume"])
        .is_err());
    assert!(cli()
        .try_get_matches_from(["logs", "demo", "--generation", "0"])
        .is_err());
    let args = cli()
        .try_get_matches_from([
            "logs",
            "demo",
            "--server",
            "http://127.0.0.1:3000",
            "--generation",
            "7",
            "--cursor",
            "resume",
            "--follow",
            "--json",
        ])
        .unwrap();
    assert_eq!(args.get_one::<u64>("generation"), Some(&7));
    assert_eq!(
        args.get_one::<String>("server").map(String::as_str),
        Some("http://127.0.0.1:3000")
    );
    assert!(args.get_flag("follow") && args.get_flag("json"));
}
