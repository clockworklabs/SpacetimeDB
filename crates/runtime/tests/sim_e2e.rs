#![cfg(feature = "simulation")]
#![allow(clippy::disallowed_macros)]

use std::{sync::Arc, time::Duration};

use futures::{
    channel::{mpsc, oneshot},
    StreamExt,
};
use spacetimedb_runtime::sim::{buggify, Rng, Runtime};
use spin::Mutex;

/// One reply produced by the simulated server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Response {
    id: u64,
    value: u64,
    at: Duration,
}

/// Trace entries recorded by the server so tests can assert schedule/fault outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServerEvent {
    Received { id: u64, at: Duration },
    Dropped { id: u64, at: Duration },
    Replied { id: u64, at: Duration },
}

/// A client request submitted to the simulated server.
struct Request {
    id: u64,
    input: u64,
    respond_to: oneshot::Sender<Response>,
}

/// Complete result of the client/server workload for one seed.
#[derive(Debug, Eq, PartialEq)]
struct ClientServerRun {
    responses: Vec<(u64, Option<Response>)>,
    server_events: Vec<ServerEvent>,
    elapsed: Duration,
}

/// Checks the "same seed, same trace" side of the client/server workload.
/// Both the client-visible results and the server-side event trace should stay
/// stable for one fixed seed.
#[test]
fn client_server_buggify_injects_deterministic_faults() {
    let run = run_buggified_client_server(404);

    assert_eq!(
        run.responses,
        vec![
            (0, None),
            (
                1,
                Some(Response {
                    id: 1,
                    value: 50,
                    at: Duration::from_millis(2),
                }),
            ),
            (
                2,
                Some(Response {
                    id: 2,
                    value: 70,
                    at: Duration::from_millis(3),
                }),
            ),
            (3, None),
            (
                4,
                Some(Response {
                    id: 4,
                    value: 110,
                    at: Duration::from_millis(5),
                }),
            ),
        ]
    );
    assert_eq!(
        run.server_events,
        vec![
            ServerEvent::Received {
                id: 3,
                at: Duration::ZERO,
            },
            ServerEvent::Received {
                id: 0,
                at: Duration::ZERO,
            },
            ServerEvent::Received {
                id: 2,
                at: Duration::ZERO,
            },
            ServerEvent::Received {
                id: 4,
                at: Duration::ZERO,
            },
            ServerEvent::Received {
                id: 1,
                at: Duration::ZERO,
            },
            ServerEvent::Dropped {
                id: 0,
                at: Duration::from_millis(1),
            },
            ServerEvent::Replied {
                id: 1,
                at: Duration::from_millis(2),
            },
            ServerEvent::Replied {
                id: 2,
                at: Duration::from_millis(3),
            },
            ServerEvent::Dropped {
                id: 3,
                at: Duration::from_millis(4),
            },
            ServerEvent::Replied {
                id: 4,
                at: Duration::from_millis(5),
            },
        ]
    );
    assert_eq!(run.elapsed, Duration::from_millis(5));
}

/// Checks the "different seed, different exploration" side of the same
/// client/server workload. The full run result should differ across seeds.
#[test]
fn client_server_buggify_differs_across_seeds() {
    let seed_404 = run_buggified_client_server(404);
    let seed_405 = run_buggified_client_server(405);

    eprintln!("seed 404: {seed_404:#?}");
    eprintln!("seed 405: {seed_405:#?}");
    assert_ne!(seed_404, seed_405);
}

/// Fixed request set used by the client workload.
const CLIENT_REQUESTS: [(u64, u64); 5] = [(0, 4), (1, 5), (2, 7), (3, 9), (4, 11)];

/// Run a small concurrent client/server workload under one seed.
///
/// The client submits every request from its own simulated task. The server
/// receives requests in scheduler order, then spawns one worker per request.
/// Each worker sleeps for deterministic virtual latency and may drop the reply
/// based on buggify.
fn run_buggified_client_server(seed: u64) -> ClientServerRun {
    // --- setup: runtime, buggify, two nodes, and communication channels ---
    let mut runtime = Runtime::new(seed);
    buggify::enable(&runtime);
    let handle = runtime.handle();
    let client_node = runtime.create_node().name("client").build();
    let server_node = runtime.create_node().name("server").build();
    // mpsc channel: client tasks send Request messages to the server task
    let (request_tx, mut request_rx) = mpsc::unbounded::<Request>();
    let server_events = Arc::new(Mutex::new(Vec::new()));

    let (responses, server_events) = runtime.block_on(async move {
        // --- server: receive 5 requests, spawn one worker per request ---
        let server_handle = handle.clone();
        let server_events_for_server = Arc::clone(&server_events);
        let server = server_node.clone().spawn(async move {
            let mut workers = Vec::new();
            // Receive all 5 requests before processing any replies
            for _ in 0..5 {
                let request = request_rx.next().await.expect("client should send request");
                server_events_for_server.lock().push(ServerEvent::Received {
                    id: request.id,
                    at: server_handle.now(),
                });

                // --- server worker: simulate latency, then drop or reply based on buggify ---
                let worker_handle = server_handle.clone();
                let worker_events = Arc::clone(&server_events_for_server);
                workers.push(server_node.clone().spawn(async move {
                    // Deterministic virtual latency: each request id has a distinct sleep
                    worker_handle.sleep(Duration::from_millis(request.id + 1)).await;
                    // buggify decides whether to drop this request (40% probability)
                    if worker_handle.buggify_with_prob(0.4) {
                        worker_events.lock().push(ServerEvent::Dropped {
                            id: request.id,
                            at: worker_handle.now(),
                        });
                        return;
                    }

                    // No fault injected: send the reply
                    let response = Response {
                        id: request.id,
                        value: request.input * 10,
                        at: worker_handle.now(),
                    };
                    worker_events.lock().push(ServerEvent::Replied {
                        id: request.id,
                        at: response.at,
                    });
                    request
                        .respond_to
                        .send(response)
                        .expect("client should wait for response");
                }));
            }

            // Wait for all server workers to complete
            for worker in workers {
                worker.await.expect("server worker should complete");
            }
        });

        // --- client: spawn one task per request, send them to server, collect responses ---
        let client_outer_node = client_node.clone();
        let client = client_node.spawn(async move {
            let mut requests = Vec::new();
            // Spawn a task for each request so they submit concurrently
            for (id, input) in CLIENT_REQUESTS {
                let request_tx = request_tx.clone();
                let client_request_node = client_outer_node.clone();
                requests.push(client_request_node.spawn(async move {
                    let (respond_to, response_rx) = oneshot::channel();
                    request_tx
                        .unbounded_send(Request { id, input, respond_to })
                        .expect("server inbox should be open");
                    // Await the server's reply (None if the server dropped this request)
                    (id, response_rx.await.ok())
                }));
            }
            // All requests sent, close the channel so the server loop terminates
            drop(request_tx);

            // Collect responses in spawn order
            let mut responses = Vec::new();
            for request in requests {
                responses.push(request.await.expect("client request task should complete"));
            }
            responses
        });

        // Drive both client and server to completion
        let responses = client.await.expect("client task should complete");
        server.await.expect("server task should complete");
        (responses, server_events.lock().clone())
    });

    // --- package the results: client responses, server trace, and total virtual time ---
    ClientServerRun {
        responses,
        server_events,
        elapsed: runtime.elapsed(),
    }
}

/// Exercises the executor, node pause/resume, and timer wheel together:
/// paused node work must not run until resumed, and all nodes must observe
/// one shared virtual clock.
#[test]
fn multi_node_runtime_coordinates_pause_resume_and_virtual_time() {
    let mut runtime = Runtime::new(101);
    let handle = runtime.handle();
    let node_a = runtime.create_node().name("a").build();
    let node_b = runtime.create_node().name("b").build();
    let events = Arc::new(Mutex::new(Vec::new()));

    node_b.pause();

    runtime.block_on({
        let events = Arc::clone(&events);
        async move {
            let a_handle = handle.clone();
            let a_events = Arc::clone(&events);
            let a = node_a.spawn(async move {
                a_events.lock().push(("a_started", a_handle.now()));
                a_handle.sleep(Duration::from_millis(3)).await;
                a_events.lock().push(("a_finished", a_handle.now()));
            });

            let b_handle = handle.clone();
            let b_events = Arc::clone(&events);
            let b = node_b.spawn(async move {
                b_events.lock().push(("b_started", b_handle.now()));
                b_handle.sleep(Duration::from_millis(2)).await;
                b_events.lock().push(("b_finished", b_handle.now()));
            });

            handle.sleep(Duration::from_millis(1)).await;
            events.lock().push(("main_resumed_b", handle.now()));
            node_b.resume();

            a.await.expect("node a task should complete");
            b.await.expect("node b task should complete");
        }
    });

    let events = events.lock().clone();
    assert!(events.contains(&("a_started", Duration::ZERO)));
    assert!(events.contains(&("main_resumed_b", Duration::from_millis(1))));
    assert!(events.contains(&("b_started", Duration::from_millis(1))));
    assert!(events.contains(&("a_finished", Duration::from_millis(3))));
    assert!(events.contains(&("b_finished", Duration::from_millis(3))));
    assert_eq!(runtime.elapsed(), Duration::from_millis(3));
}

/// Checks that runtime-owned buggify decisions consume the same seeded RNG
/// sequence as an explicit `Rng`, making injected faults replayable by seed.
#[test]
fn runtime_buggify_matches_standalone_rng_sequence() {
    let seed = 77;
    let runtime = Runtime::new(seed);
    let expected = Rng::new(seed);

    buggify::enable(&runtime);
    expected.enable_buggify();

    let actual = (0..8)
        .map(|_| buggify::should_inject_fault_with_prob(&runtime, 0.5))
        .collect::<Vec<_>>();
    let expected = (0..8).map(|_| expected.buggify_with_prob(0.5)).collect::<Vec<_>>();

    assert_eq!(actual, expected);
    assert!(buggify::is_enabled(&runtime));

    buggify::disable(&runtime);
    assert!(!buggify::is_enabled(&runtime));
    assert!(!buggify::should_inject_fault_with_prob(&runtime, 1.0));
}

/// Verifies timeout races are driven by virtual time, not wall time: the fast
/// node completes at 2ms, then the slow node times out at the shared 4ms
/// deadline.
#[test]
fn multi_node_timeout_uses_shared_virtual_clock() {
    let mut runtime = Runtime::new(303);
    let handle = runtime.handle();
    let slow_node = runtime.create_node().name("slow").build();
    let fast_node = runtime.create_node().name("fast").build();

    let output = runtime.block_on(async move {
        let slow_handle = handle.clone();
        let slow = slow_node.spawn(async move {
            slow_handle
                .timeout(Duration::from_millis(4), async {
                    slow_handle.sleep(Duration::from_millis(10)).await;
                    "slow-finished"
                })
                .await
        });

        let fast_handle = handle.clone();
        let fast = fast_node.spawn(async move {
            fast_handle.sleep(Duration::from_millis(2)).await;
            ("fast-finished", fast_handle.now())
        });

        (
            slow.await.expect("slow node task should complete"),
            fast.await.expect("fast node task should complete"),
        )
    });

    let (slow, fast) = output;
    assert_eq!(fast, ("fast-finished", Duration::from_millis(2)));
    assert_eq!(slow.unwrap_err().duration(), Duration::from_millis(4));
    assert_eq!(runtime.elapsed(), Duration::from_millis(4));
}
