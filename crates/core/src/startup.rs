use itertools::Itertools;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing_appender::rolling;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::{reload, EnvFilter};

pub use crate::host::host_controller::{init as init_host, init_basic as init_host_basic};

pub fn configure_logging() {
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
    let filter = parse_from_file(&conf_file);
    let builder = tracing_subscriber::fmt()
        .with_writer(std::io::stdout.and(rolling::daily("/var/log", "spacetimedb.log")))
        .with_line_number(true)
        .with_file(true)
        .with_target(false)
        .with_env_filter(filter);

    if cfg!(debug_assertions) {
        let builder = builder.with_filter_reloading();
        let reload_handle = builder.reload_handle();
        std::thread::spawn(move || reload_config(&conf_file, &reload_handle));
        builder.init()
    } else {
        builder.init()
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
