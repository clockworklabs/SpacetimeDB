#![cfg(feature = "simulation")]

use std::{sync::Arc, time::Duration};

use futures::{
    channel::{mpsc, oneshot},
    StreamExt,
};
use spacetimedb_runtime::sim::{buggify, Rng, Runtime};
use spin::Mutex;

#[test]
fn multi_node_runtime_coordinates_pause_resume_and_virtual_time() {
    // Exercises the executor, node pause/resume, and timer wheel together:
    // paused node work must not run until resumed, and all nodes must observe
    // one shared virtual clock.
    let mut runtime = Runtime::new(101);
    let handle = runtime.handle();
    let node_a = runtime.create_node();
    let node_b = runtime.create_node();
    let events = Arc::new(Mutex::new(Vec::new()));

    runtime.pause(node_b);

    runtime.block_on({
        let events = Arc::clone(&events);
        async move {
            let a_handle = handle.clone();
            let a_events = Arc::clone(&events);
            let a = handle.spawn_on(node_a, async move {
                a_events.lock().push(("a_started", a_handle.now()));
                a_handle.sleep(Duration::from_millis(3)).await;
                a_events.lock().push(("a_finished", a_handle.now()));
            });

            let b_handle = handle.clone();
            let b_events = Arc::clone(&events);
            let b = handle.spawn_on(node_b, async move {
                b_events.lock().push(("b_started", b_handle.now()));
                b_handle.sleep(Duration::from_millis(2)).await;
                b_events.lock().push(("b_finished", b_handle.now()));
            });

            handle.sleep(Duration::from_millis(1)).await;
            events.lock().push(("main_resumed_b", handle.now()));
            handle.resume(node_b);

            a.await;
            b.await;
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

#[test]
fn client_server_request_response_uses_virtual_time() {
    // Models a small client/server exchange without real networking: the client
    // sends requests over an in-memory channel, and the server replies after
    // deterministic virtual latency on a different simulated node.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Response {
        id: u64,
        value: u64,
        at: Duration,
    }

    struct Request {
        id: u64,
        input: u64,
        respond_to: oneshot::Sender<Response>,
    }

    let mut runtime = Runtime::new(404);
    let handle = runtime.handle();
    let client_node = runtime.create_node();
    let server_node = runtime.create_node();
    let (request_tx, mut request_rx) = mpsc::unbounded::<Request>();

    let responses = runtime.block_on(async move {
        let server_handle = handle.clone();
        let server = handle.spawn_on(server_node, async move {
            for _ in 0..3 {
                let request = request_rx.next().await.expect("client should send request");
                server_handle.sleep(Duration::from_millis(request.id + 1)).await;
                request
                    .respond_to
                    .send(Response {
                        id: request.id,
                        value: request.input * 10,
                        at: server_handle.now(),
                    })
                    .expect("client should wait for response");
            }
        });

        let client = handle.spawn_on(client_node, async move {
            let mut responses = Vec::new();
            for (id, input) in [(2, 7), (0, 4), (1, 5)] {
                let (respond_to, response_rx) = oneshot::channel();
                request_tx
                    .unbounded_send(Request { id, input, respond_to })
                    .expect("server inbox should be open");
                responses.push(response_rx.await.expect("server should reply"));
            }
            responses
        });

        let responses = client.await;
        server.await;
        responses
    });

    assert_eq!(
        responses,
        vec![
            Response {
                id: 2,
                value: 70,
                at: Duration::from_millis(3)
            },
            Response {
                id: 0,
                value: 40,
                at: Duration::from_millis(4)
            },
            Response {
                id: 1,
                value: 50,
                at: Duration::from_millis(6)
            },
        ]
    );
    assert_eq!(runtime.elapsed(), Duration::from_millis(6));
}

#[test]
fn runtime_buggify_matches_standalone_rng_sequence() {
    // Checks that runtime-owned buggify decisions consume the same seeded RNG
    // sequence as an explicit `Rng`, making injected faults replayable by seed.
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

#[test]
fn multi_node_timeout_uses_shared_virtual_clock() {
    // Verifies timeout races are driven by virtual time, not wall time: the
    // fast node completes at 2ms, then the slow node times out at the shared
    // 4ms deadline.
    let mut runtime = Runtime::new(303);
    let handle = runtime.handle();
    let slow_node = runtime.create_node();
    let fast_node = runtime.create_node();

    let output = runtime.block_on(async move {
        let slow_handle = handle.clone();
        let slow = handle.spawn_on(slow_node, async move {
            slow_handle
                .timeout(Duration::from_millis(4), async {
                    slow_handle.sleep(Duration::from_millis(10)).await;
                    "slow-finished"
                })
                .await
        });

        let fast_handle = handle.clone();
        let fast = handle.spawn_on(fast_node, async move {
            fast_handle.sleep(Duration::from_millis(2)).await;
            ("fast-finished", fast_handle.now())
        });

        (slow.await, fast.await)
    });

    let (slow, fast) = output;
    assert_eq!(fast, ("fast-finished", Duration::from_millis(2)));
    assert_eq!(slow.unwrap_err().duration(), Duration::from_millis(4));
    assert_eq!(runtime.elapsed(), Duration::from_millis(4));
}
