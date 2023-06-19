use itertools::Itertools;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing_appender::rolling;
use tracing_flame::FlameLayer;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{reload, EnvFilter};

pub fn configure_tracing() {
    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.
    let conf_file = std::env::var_os("SPACETIMEDB_LOG_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let local = Path::new("log.conf");
            if local.exists() {
                local.to_owned()
            } else {
                "/etc/spacetimedb/log.conf".into()
            }
        });
    let logs_path: String = std::env::var("SPACETIMEDB_LOGS_PATH").unwrap_or("/var/log".into());

    let timer = tracing_subscriber::fmt::time();
    let format = tracing_subscriber::fmt::format::Format::default()
        .with_timer(timer)
        .with_line_number(true)
        .with_file(true)
        .with_target(false)
        .compact();

    let fmt_layer = tracing_subscriber::fmt::Layer::default()
        .with_writer(std::io::stdout.and(rolling::daily(logs_path, "spacetimedb.log")))
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

    let subscriber = tracing_subscriber::Registry::default()
        .with(fmt_layer)
        .with(tracy_layer)
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
                eprintln!("reloading log config...");
                prev_time = Some(modified);
                if reload_handle.reload(parse_from_file(conf_file)).is_err() {
                    break;
                }
            }
        }
    }
}
