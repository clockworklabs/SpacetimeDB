use crate::modules::start_runtime;
use reqwest::blocking::Client;
use spacetimedb::util::jobs::JobCores;
use std::{
    net::SocketAddr,
    thread::{sleep, JoinHandle},
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn find_free_port() -> u16 {
    portpicker::pick_unused_port().expect("no free ports available")
}

pub struct SpacetimeDbGuard {
    thread: Option<JoinHandle<()>>,
    pub host_url: String,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

// Remove all Cargo-provided env vars from a child process. These are set by the fact that we're running in a cargo
// command (e.g. `cargo test`). We don't want to inherit any of these to a child cargo process, because it causes
// unnecessary rebuilds.
impl SpacetimeDbGuard {
    pub fn spawn_in_temp_data_dir() -> Self {
        Self::spawn_spacetime_start(&[])
    }

    fn run_spacetime(args: Vec<String>, cancel: tokio::sync::oneshot::Receiver<()>, _temp_dir: TempDir) {
        let runtime = start_runtime();
        let args = spacetimedb_standalone::start::cli().try_get_matches_from(args).unwrap();
        runtime.block_on(async {
            tokio::select! {
                _ = spacetimedb_standalone::start::exec(&args, JobCores::without_pinned_cores(runtime.handle().clone())) => (),
                _ = cancel => (),
            }
        });
    }

    fn spawn_spacetime_start(extra_args: &[&str]) -> Self {
        let port = find_free_port();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let address = addr.to_string();
        let host_url = format!("http://{}", addr);

        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let data_dir = temp_dir.path().display().to_string();

        let mut args = vec![
            "start".to_string(),
            "--data-dir".to_string(),
            data_dir.to_string(),
            "--listen-addr".to_string(),
            address,
        ];

        args.extend(extra_args.iter().map(ToString::to_string));

        let (cancel, cancel_recv) = tokio::sync::oneshot::channel();
        let thread = std::thread::spawn(move || {
            Self::run_spacetime(args, cancel_recv, temp_dir);
        });

        let guard = SpacetimeDbGuard {
            thread: Some(thread),
            cancel: Some(cancel),
            host_url,
        };
        guard.wait_until_http_ready(Duration::from_secs(10));
        guard
    }

    fn wait_until_http_ready(&self, timeout: Duration) {
        let client = Client::new();
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            let url = format!("{}/v1/ping", self.host_url);

            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    return; // Fully ready!
                }
            }

            sleep(Duration::from_millis(50));
        }
        panic!("Timed out waiting for SpacetimeDB HTTP /v1/ping at {}", self.host_url);
    }
}

impl Drop for SpacetimeDbGuard {
    fn drop(&mut self) {
        let _ = self.cancel.take().unwrap().send(());
        if let Err(e) = self.thread.take().unwrap().join() {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s
            } else if let Some(s) = e.downcast_ref::<&str>() {
                s
            } else {
                "dyn Any"
            };
            panic!("standalone process failed by panic: {msg}")
        }
    }
}
