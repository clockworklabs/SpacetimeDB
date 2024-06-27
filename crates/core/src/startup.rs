use itertools::Itertools;
use opentelemetry_otlp::WithExportConfig;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use tokio::fs::File;
use tonic::transport::{Certificate, Channel, Endpoint, Uri};
use tracing_appender::rolling;
use tracing_core::{Metadata, Subscriber};
use tracing_flame::FlameLayer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::{Filter, SubscriberExt};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{reload, EnvFilter};

pub struct StartupOptions {
    /// Whether or not to configure the global tracing subscriber.
    pub tracing: bool,
    /// Whether or not to configure the global rayon threadpool.
    pub rayon: bool,
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            tracing: true,
            rayon: true,
        }
    }
}

impl StartupOptions {
    pub async fn configure(self) {
        if self.tracing {
            configure_tracing().await
        }
        if self.rayon {
            configure_rayon()
        }
    }
}

use opentelemetry::trace::{Tracer, TracerProvider as _};
use opentelemetry_sdk::trace::TracerProvider;
use tonic::metadata::MetadataValue;
use tonic::transport::channel::ClientTlsConfig;
use tracing::{error, span};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Registry;

struct EventTracesFilter;

impl<S> Filter<S> for EventTracesFilter
where
    S: Subscriber,
{
    fn enabled(&self, metadata: &Metadata<'_>, ctx: &Context<'_, S>) -> bool {
        // Check if the span has the 'event_trace' field set to true
        metadata.fields().iter().any(|field| field.name() == "event_trace")
    }
}

async fn configure_tracing() {
    let mut metadata = tonic::metadata::MetadataMap::new();
    let honeycomb_api_key = std::env::var("HONEYCOMB_API_KEY").unwrap();
    metadata.insert("x-honeycomb-team", MetadataValue::from_str(&honeycomb_api_key).unwrap());
    let honeycomb_ca_path = std::env::var("HONEYCOMB_CA_PATH").unwrap_or("/etc/spacetimedb/honeycomb_ca.pem".into());
    let pem = fs::read_to_string(&honeycomb_ca_path).expect("Could not read honeycomb cert");
    let certificate = Certificate::from_pem(pem.as_bytes());
    let tls_config = ClientTlsConfig::default()
        .domain_name("api.honeycomb.io")
        .ca_certificate(certificate)
        .assume_http2(false);
    let channel = Channel::builder(Uri::from_static("https://api.honeycomb.io:443"))
        .tls_config(tls_config)
        .expect("Could not set up TLS for telemetry GRPC")
        .tls_assume_http2(false)
        .connect()
        .await
        .expect("Could not connect");
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_metadata(metadata)
        .with_channel(channel);
    let provider = TracerProvider::builder().build();
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .expect("Couldn't set up tracer");

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // global::set_tracer_provider(provider);

    // tracing_subscriber::registry().with(telemetry).try_init()?;

    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let conf_file = std::env::var_os("SPACETIMEDB_LOG_CONFIG")
        .map(PathBuf::from)
        .expect("SPACETIMEDB_LOG_CONFIG must be set to a valid path to a log config file");
    let logs_path: String = std::env::var("SPACETIMEDB_LOGS_PATH")
        .expect("SPACETIMEDB_LOGS_PATH must be set to a valid path to a log directory");

    let timer = tracing_subscriber::fmt::time();
    let format = tracing_subscriber::fmt::format::Format::default()
        .with_timer(timer)
        .with_line_number(true)
        .with_file(true)
        .with_target(false)
        .compact();

    let disable_disk_logging = std::env::var_os("SPACETIMEDB_DISABLE_DISK_LOGGING").is_some();

    let write_to = if disable_disk_logging {
        BoxMakeWriter::new(std::io::stdout)
    } else {
        BoxMakeWriter::new(std::io::stdout.and(rolling::daily(logs_path, "spacetimedb.log")))
    };

    let fmt_layer = tracing_subscriber::fmt::Layer::default()
        .with_writer(write_to)
        .event_format(format);

    let env_filter_layer = parse_from_file(&conf_file);

    let tracy_layer = if std::env::var("SPACETIMEDB_TRACY").is_ok() {
        Some(tracing_tracy::TracyLayer::new())
    } else {
        None
    };

    let (flame_guard, flame_layer) = if std::env::var("SPACETIMEDB_FLAMEGRAPH").is_ok() {
        let flamegraph_path =
            std::env::var("SPACETIMEDB_FLAMEGRAPH_PATH").unwrap_or("/var/log/flamegraph.folded".into());
        let (flame_layer, guard) = FlameLayer::with_file(flamegraph_path).unwrap();
        let flame_layer = flame_layer.with_file_and_line(false).with_empty_samples(false);
        (Some(guard), Some(flame_layer))
    } else {
        (None, None)
    };

    // Is important for `tracy_layer` to be before `fmt_layer` to not print ascii codes...
    let subscriber = tracing_subscriber::Registry::default()
        .with(tracy_layer)
        .with(telemetry_layer)
        .with(fmt_layer)
        .with(flame_layer);

    if cfg!(debug_assertions) {
        let (reload_layer, reload_handle) = tracing_subscriber::reload::Layer::new(env_filter_layer);
        std::thread::spawn(move || reload_config(&conf_file, &reload_handle));
        subscriber.with(reload_layer).init();
    } else {
        subscriber.with(env_filter_layer).init();
    };

    if let Some(guard) = flame_guard {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                guard.flush().unwrap();
            }
        });
    }
}

fn parse_from_file(file: &Path) -> EnvFilter {
    let conf = std::fs::read_to_string(file).unwrap_or_default();
    let directives = conf
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .join(",");
    EnvFilter::new(directives)
}

const RELOAD_INTERVAL: Duration = Duration::from_secs(5);
fn reload_config<S>(conf_file: &Path, reload_handle: &reload::Handle<EnvFilter, S>) {
    let mut prev_time = conf_file.metadata().and_then(|m| m.modified()).ok();
    loop {
        std::thread::sleep(RELOAD_INTERVAL);
        if let Ok(modified) = conf_file.metadata().and_then(|m| m.modified()) {
            if prev_time.map_or(true, |prev| modified > prev) {
                log::info!("reloading log config...");
                prev_time = Some(modified);
                if reload_handle.reload(parse_from_file(conf_file)).is_err() {
                    break;
                }
            }
        }
    }
}

fn configure_rayon() {
    rayon_core::ThreadPoolBuilder::new()
        .thread_name(|_idx| "rayon-worker".to_string())
        .spawn_handler(thread_spawn_handler(tokio::runtime::Handle::current()))
        // TODO(perf, pgoldman 2024-02-22):
        // in the case where we have many modules running many reducers,
        // we'll wind up with Rayon threads competing with each other and with Tokio threads
        // for CPU time.
        //
        // We should investigate creating two separate CPU pools,
        // possibly via https://docs.rs/nix/latest/nix/sched/fn.sched_setaffinity.html,
        // and restricting Tokio threads to one CPU pool
        // and Rayon threads to the other.
        // Then we should give Tokio and Rayon each a number of worker threads
        // equal to the size of their pool.
        .num_threads(std::thread::available_parallelism().unwrap().get() / 2)
        .build_global()
        .unwrap()
}

/// A Rayon [spawn_handler](https://docs.rs/rustc-rayon-core/latest/rayon_core/struct.ThreadPoolBuilder.html#method.spawn_handler)
/// which enters the given Tokio runtime at thread startup,
/// so that the Rayon workers can send along async channels.
///
/// Other than entering the `rt`, this spawn handler behaves identitically to the default Rayon spawn handler,
/// as documented in
/// https://docs.rs/rustc-rayon-core/0.5.0/rayon_core/struct.ThreadPoolBuilder.html#method.spawn_handler
///
/// Having Rayon threads block on async operations is a code smell.
/// We need to be careful that the Rayon threads never actually block,
/// i.e. that every async operation they invoke immediately completes.
/// I (pgoldman 2024-02-22) believe that our Rayon threads only ever send to unbounded channels,
/// and therefore never wait.
fn thread_spawn_handler(rt: tokio::runtime::Handle) -> impl FnMut(rayon::ThreadBuilder) -> Result<(), std::io::Error> {
    move |thread| {
        let rt = rt.clone();
        let mut builder = std::thread::Builder::new();
        if let Some(name) = thread.name() {
            builder = builder.name(name.to_owned());
        }
        if let Some(stack_size) = thread.stack_size() {
            builder = builder.stack_size(stack_size);
        }
        builder.spawn(move || {
            let _rt_guard = rt.enter();
            thread.run()
        })?;
        Ok(())
    }
}
