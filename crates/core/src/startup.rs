use core_affinity::CoreId;
use crossbeam_queue::ArrayQueue;
use spacetimedb_paths::server::{ConfigToml, LogsDir};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_appender::rolling;
use tracing_core::LevelFilter;
use tracing_flame::FlameLayer;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{reload, EnvFilter};

use crate::config::{ConfigFile, LogConfig};
use crate::util::jobs::JobCores;

pub struct TracingOptions {
    pub config: LogConfig,
    /// Whether or not to periodically reload the log config in the background.
    pub reload_config: Option<ConfigToml>,
    /// Whether or not to write logs to disk.
    pub disk_logging: Option<LogsDir>,
    /// The edition of this spacetime server.
    pub edition: String,
    /// Enables tracy profiling.
    pub tracy: bool,
    pub flamegraph: Option<PathBuf>,
}

impl Default for TracingOptions {
    fn default() -> Self {
        Self {
            config: LogConfig::default(),
            reload_config: None,
            disk_logging: None,
            edition: "standalone".to_owned(),
            tracy: false,
            flamegraph: None,
        }
    }
}
#[must_use]
pub fn pin_threads() -> Cores {
    Cores::get().unwrap_or_default()
}

pub fn configure_tracing(opts: TracingOptions) {
    // Use this to change log levels at runtime.
    // This means you can change the default log level to trace
    // if you are trying to debug an issue and need more logs on then turn it off
    // once you are done.

    let timer = tracing_subscriber::fmt::time();
    let format = tracing_subscriber::fmt::format::Format::default()
        .with_timer(timer)
        .with_line_number(true)
        .with_file(true)
        .with_target(false)
        .compact();

    let write_to = if let Some(logs_dir) = opts.disk_logging {
        let roller = rolling::Builder::new()
            .filename_prefix(LogsDir::filename_prefix(&opts.edition))
            .filename_suffix(LogsDir::filename_extension())
            .build(logs_dir)
            .unwrap();
        // TODO: syslog?
        BoxMakeWriter::new(std::io::stdout.and(roller))
    } else {
        BoxMakeWriter::new(std::io::stdout)
    };

    let fmt_layer = tracing_subscriber::fmt::Layer::default()
        .with_writer(write_to)
        .event_format(format);

    let env_filter_layer = conf_to_filter(opts.config);

    let tracy_layer = if opts.tracy {
        Some(tracing_tracy::TracyLayer::new())
    } else {
        None
    };

    let (flame_guard, flame_layer) = if let Some(flamegraph_path) = opts.flamegraph {
        let (flame_layer, guard) = FlameLayer::with_file(flamegraph_path).unwrap();
        let flame_layer = flame_layer.with_file_and_line(false).with_empty_samples(false);
        (Some(guard), Some(flame_layer))
    } else {
        (None, None)
    };

    // Is important for `tracy_layer` to be before `fmt_layer` to not print ascii codes...
    let subscriber = tracing_subscriber::Registry::default()
        .with(tracy_layer)
        .with(fmt_layer)
        .with(flame_layer);

    if let Some(conf_file) = opts.reload_config {
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

fn conf_to_filter(conf: LogConfig) -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(conf.level.unwrap_or(LevelFilter::ERROR).into())
        .parse_lossy(conf.directives.join(","))
}

fn parse_from_file(path: &ConfigToml) -> EnvFilter {
    let conf = match ConfigFile::read(path) {
        Ok(Some(conf)) => conf.logs,
        Ok(None) => LogConfig::default(),
        #[allow(clippy::disallowed_macros)]
        Err(e) => {
            eprintln!("error reading config.toml for logconf reloading: {e}");
            LogConfig::default()
        }
    };
    conf_to_filter(conf)
}

const RELOAD_INTERVAL: Duration = Duration::from_secs(5);
fn reload_config<S>(conf_file: &ConfigToml, reload_handle: &reload::Handle<EnvFilter, S>) {
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

pub struct Cores {
    pub databases: JobCores,
    pub tokio_workers: TokioCores,
    pub rest: usize,
}

impl Default for Cores {
    fn default() -> Self {
        Self {
            databases: JobCores::default(),
            tokio_workers: TokioCores(None),
            rest: std::thread::available_parallelism().map_or(4, |x| x.get()),
        }
    }
}

impl Cores {
    fn get() -> Option<Self> {
        let cores = &mut core_affinity::get_core_ids()
            .filter(|cores| cores.len() >= 8)?
            .into_iter();

        let total = cores.len() as f64;
        let frac = |frac: f64| (total * frac).ceil() as usize;

        let databases = cores.take(frac(1.0 / 8.0)).collect();

        let tokio_workers = TokioCores(Some(cores.take(frac(5.0 / 8.0)).collect()));

        Some(Self {
            databases,
            tokio_workers,
            rest: cores.len(),
        })
    }
}

type CoreQueue = Arc<ArrayQueue<CoreId>>;
fn vec_to_queue(cores: Vec<CoreId>) -> CoreQueue {
    let queue = Arc::new(ArrayQueue::new(cores.len()));
    for core in cores {
        queue.push(core).unwrap();
    }
    queue
}

pub struct TokioCores(Option<Vec<CoreId>>);

impl TokioCores {
    pub fn configure(self, builder: &mut tokio::runtime::Builder) {
        if let Some(cores) = self.0 {
            let cores = vec_to_queue(cores);
            // `on_thread_start` gets called for both async worker threads and blocking threads,
            // but the first `worker_threads` threads that tokio spawns are worker threads,
            // so this ends up working fine
            builder.worker_threads(cores.len()).on_thread_start(move || {
                if let Some(core) = cores.pop() {
                    core_affinity::set_for_current(core);
                }
            });
        }
    }
}

pub fn configure_rayon(num_threads: usize, tokio_handle: &tokio::runtime::Handle) {
    rayon_core::ThreadPoolBuilder::new()
        .thread_name(|_idx| "rayon-worker".to_string())
        .spawn_handler(thread_spawn_handler(tokio_handle))
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
        .num_threads(num_threads)
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
fn thread_spawn_handler(
    rt: &tokio::runtime::Handle,
) -> impl FnMut(rayon::ThreadBuilder) -> Result<(), std::io::Error> + '_ {
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
