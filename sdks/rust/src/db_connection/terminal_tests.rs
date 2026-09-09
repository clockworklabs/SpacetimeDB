use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "../../tests/connect_disconnect_client/src/module_bindings/mod.rs"]
pub(super) mod bindings;
use bindings::RemoteModule;

#[derive(spacetimedb_lib::ser::Serialize)]
#[sats(crate = spacetimedb_lib)]
struct Args {}
impl InModule for Args {
    type Module = RemoteModule;
}
impl From<Args> for bindings::Reducer {
    fn from(_: Args) -> Self {
        Self::IdentityConnected
    }
}

struct DropProbe {
    context: DbContextImpl<RemoteModule>,
    drops: Arc<AtomicUsize>,
}
impl Drop for DropProbe {
    fn drop(&mut self) {
        assert!(
            self.context.inner.try_lock().is_ok(),
            "capture dropped under inner lock"
        );
        assert!(
            self.context.send_chan.try_lock().is_ok(),
            "capture dropped under send lock"
        );
        assert!(
            self.context.pending_mutations_recv.try_lock().is_ok(),
            "capture dropped under queue lock"
        );
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn fixture(
    runtime: &Runtime,
    disconnects: Arc<AtomicUsize>,
) -> (
    DbContextImpl<RemoteModule>,
    mpsc::UnboundedSender<ParsedMessage<RemoteModule>>,
) {
    let inner = build_db_ctx_inner::<RemoteModule>(
        None,
        NativeTasks::default(),
        None,
        None,
        Some(Box::new(move |_, error| {
            assert!(error.is_none());
            disconnects.fetch_add(1, Ordering::SeqCst);
        })),
    );
    inner.lock().unwrap().connection_lifecycle = ConnectionLifecycle::Connected;
    let (outgoing, outgoing_recv) = mpsc::unbounded();
    // Keep the transport receiver alive without any network or server fixture.
    let (incoming, incoming_recv) = mpsc::unbounded();
    let (pending, pending_recv) = mpsc::unbounded();
    let context = build_db_ctx(
        runtime.handle().clone(),
        inner,
        outgoing,
        Arc::new(TokioMutex::new(incoming_recv)),
        pending,
        Arc::new(TokioMutex::new(pending_recv)),
        Some(ConnectionId::from_u128(1)),
        None,
    );
    runtime.spawn(async move {
        let mut outgoing_recv = outgoing_recv;
        while outgoing_recv.next().await.is_some() {}
    });
    (context, incoming)
}

fn queue_calls(context: &DbContextImpl<RemoteModule>, drops: &Arc<AtomicUsize>) {
    let reducer = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    context
        .invoke_reducer_with_callback(Args {}, move |_, _| {
            let _capture = reducer;
            panic!("unknown-outcome reducer must not report completion");
        })
        .unwrap();
    let procedure = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    context.invoke_procedure_with_callback::<_, ()>("procedure", Args {}, move |_, _| {
        let _capture = procedure;
        panic!("unknown-outcome procedure must not report completion");
    });
}

#[test]
fn terminal_disconnect_releases_inflight_and_queued_requests_with_retained_context() {
    let runtime = Runtime::new().unwrap();
    let disconnects = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let (context, _incoming) = fixture(&runtime, disconnects.clone());
    queue_calls(&context, &drops);
    context.frame_tick().unwrap();
    queue_calls(&context, &drops);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(matches!(context.end_connection(None), crate::Error::Disconnected));
    assert_eq!(drops.load(Ordering::SeqCst), 4);
    assert_eq!(disconnects.load(Ordering::SeqCst), 1);
    assert!(!context.is_active());

    let late = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    assert!(matches!(
        context.invoke_reducer_with_callback(Args {}, move |_, _| drop(late)),
        Err(crate::Error::Disconnected)
    ));
    let late = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    context.invoke_procedure_with_callback::<_, ()>("procedure", Args {}, move |_, _| drop(late));
    assert_eq!(drops.load(Ordering::SeqCst), 6);
    context.end_connection(None);
    assert_eq!(disconnects.load(Ordering::SeqCst), 1);
}

#[test]
fn queued_disconnect_releases_calls_and_drains_late_results_until_terminal_event() {
    let runtime = Runtime::new().unwrap();
    let disconnects = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let (context, incoming) = fixture(&runtime, disconnects.clone());
    queue_calls(&context, &drops);
    context.frame_tick().unwrap();
    context.disconnect().unwrap();
    queue_calls(&context, &drops);
    incoming
        .unbounded_send(ParsedMessage::ReducerResult {
            request_id: u32::MAX,
            timestamp: Timestamp::UNIX_EPOCH,
            result: Ok(Ok(bindings::DbUpdate::default())),
        })
        .unwrap();
    incoming
        .unbounded_send(ParsedMessage::ProcedureResult {
            request_id: u32::MAX,
            result: Ok(Bytes::new()),
        })
        .unwrap();
    context.frame_tick().unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 4);
    assert_eq!(
        disconnects.load(Ordering::SeqCst),
        0,
        "local request is not the terminal callback"
    );
    assert!(!context.is_active());
    drop(incoming);
    assert!(matches!(
        runtime.block_on(context.advance_one_message_async()),
        Err(crate::Error::Disconnected)
    ));
    assert_eq!(disconnects.load(Ordering::SeqCst), 1);
}

#[test]
fn retained_table_and_subscription_handles_cannot_retain_callbacks_after_terminal() {
    let runtime = Runtime::new().unwrap();
    let disconnects = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let (context, _incoming) = fixture(&runtime, disconnects);
    let table = context.get_table::<bindings::Connected>("connected");
    let capture = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    table.on_insert(move |_, _| {
        let _ = &capture;
    });
    let capture = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    let registered = crate::subscription::SubscriptionBuilder::<RemoteModule>::new(&context)
        .on_applied(move |_| drop(capture))
        .subscribe("SELECT * FROM connected");
    context.frame_tick().unwrap();
    let capture = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    let queued = crate::subscription::SubscriptionBuilder::<RemoteModule>::new(&context)
        .on_applied(move |_| drop(capture))
        .subscribe("SELECT * FROM connected");
    context.end_connection(None);
    assert_eq!(drops.load(Ordering::SeqCst), 3);
    assert!(crate::spacetime_module::SubscriptionHandle::is_ended(&registered));
    assert!(crate::spacetime_module::SubscriptionHandle::is_ended(&queued));
    let capture = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    table.on_insert(move |_, _| {
        let _ = &capture;
    });
    let capture = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    let subscription = crate::subscription::SubscriptionBuilder::<RemoteModule>::new(&context)
        .on_applied(move |_| drop(capture))
        .subscribe("SELECT * FROM connected");
    assert_eq!(drops.load(Ordering::SeqCst), 5);
    assert!(crate::spacetime_module::SubscriptionHandle::is_ended(&subscription));
    assert_eq!(table.iter().count(), 0);
}

#[test]
fn cancelling_before_initial_message_releases_registered_subscription_without_callbacks() {
    let runtime = Runtime::new().unwrap();
    let disconnects = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let (context, incoming) = fixture(&runtime, disconnects.clone());
    context.inner.lock().unwrap().connection_lifecycle = ConnectionLifecycle::Connecting;
    let capture = DropProbe {
        context: context.clone(),
        drops: drops.clone(),
    };
    let subscription = crate::subscription::SubscriptionBuilder::<RemoteModule>::new(&context)
        .on_applied(move |_| drop(capture))
        .subscribe("SELECT * FROM connected");
    context.frame_tick().unwrap();
    context.disconnect().unwrap();
    context.frame_tick().unwrap();
    drop(incoming);
    assert!(matches!(
        runtime.block_on(context.advance_one_message_async()),
        Err(crate::Error::Disconnected)
    ));
    assert_eq!(disconnects.load(Ordering::SeqCst), 0);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(crate::spacetime_module::SubscriptionHandle::is_ended(&subscription));
}
