use core_affinity::CoreId;
use crossbeam_queue::ArrayQueue;
use itertools::Itertools;
use spacetimedb_paths::server::{ConfigToml, LogsDir};
use std::num::NonZeroUsize;
use std::path::PathBuf;
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

/// Divide up the available CPU cores into pools for different purposes.
///
/// Use the fields of the returned [`Cores`] value to actually configure
/// cores to be pinned.
///
/// Pinning different subsystems to different threads reduces overhead from
/// unnecessary context switching.
///
/// * Database instances are critical to overall performance, and keeping each
///   one on only one thread was shown to significantly increase transaction throughput.
/// * Tokio and Rayon have their own userspace task schedulers, so if the OS
///   scheduler is trying to schedule threads as well, it's likely to just
///   cause interference.
///
/// Call only once per process. If obtaining the number of cores fails, or if
/// there are too few cores, this function may return `Cores::default()`, which
/// performs no thread pinning.
// TODO: pinning threads might not be desirable on a machine with other
//       processes running - this should probably be some sort of flag.
#[must_use]
pub fn pin_threads() -> Cores {
    Cores::get(0).unwrap_or_default()
}

/// Like [`pin_threads`], but reserve up to `reserved_cores` for other uses.
///
/// The reservation is not guaranteed, it will be obtained after the CPU cores
/// for I/O, databases, tokio and rayon have been allocated.
///
/// Call only once per process, mutually exclusive with [`pin_threads`].
#[must_use]
pub fn pin_threads_with_reserved_cores(reserved_cores: NonZeroUsize) -> Cores {
    Cores::get(reserved_cores.get()).unwrap_or_default()
}

/// A type holding cores divvied up into different sets.
///
/// Obtained from [`pin_threads()`].
#[derive(Default)]
pub struct Cores {
    /// The cores to run database instances on.
    ///
    /// Currently, this is 1/8 of num_cpus.
    pub databases: JobCores,
    /// The cores to run tokio worker and blocking threads on.
    ///
    /// Currently, tokio worker threads are 4/8 of num_cpus, and tokio blocking
    /// threads are pinned non-exclusively to 2/8 of num_cpus.
    pub tokio: TokioCores,
    /// The cores to run rayon threads on.
    ///
    /// Currently, this is 1/8 of num_cpus.
    pub rayon: RayonCores,
    /// Extra cores if [`Self::get`] was called with a non-zero argument.
    ///
    /// `None` if no reservation could be made, otherwise `Some` containing a
    /// non-empty vector of up to the requested number of cores.
    pub reserved: Option<Vec<CoreId>>,
    /// Cores shared between tokio runtimes to schedule blocking tasks on.
    ///
    /// See `Tokio.blocking` for more context.
    #[cfg(target_os = "linux")]
    pub blocking: Option<nix::sched::CpuSet>,
}

impl Cores {
    fn get(reserve: usize) -> Option<Self> {
        let cores = &mut core_affinity::get_core_ids()
            .filter(|cores| cores.len() >= 10)?
            .into_iter()
            // We reserve the first two cores for the OS.
            // This allows us to pin interrupt handlers (IRQs) to these cores,
            // particularly those for incoming network traffic,
            // preventing them from preempting the main reducer threads.
            .filter(|core_id| core_id.id > 1)
            .collect_vec()
            .into_iter();

        let total = cores.len() as f64;
        let frac = |frac: f64| (total * frac).ceil() as usize;

        let databases = cores.take(frac(1.0 / 8.0)).collect();

        let tokio_workers = cores.take(frac(4.0 / 8.0)).collect();

        let rayon = RayonCores(Some(cores.take(frac(1.0 / 8.0)).collect()));

        let reserved = {
            let reserved = cores.take(reserve).collect_vec();
            (!reserved.is_empty()).then_some(reserved)
        };

        // see comment on `TokioCores.blocking`
        #[cfg(target_os = "linux")]
        let remaining = cores.try_fold(nix::sched::CpuSet::new(), |mut cpuset, core| {
            cpuset.set(core.id).ok()?;
            Some(cpuset)
        });

        let tokio = TokioCores {
            workers: Some(tokio_workers),
            #[cfg(target_os = "linux")]
            blocking: remaining,
        };

        Some(Self {
            databases,
            tokio,
            rayon,
            reserved,
            #[cfg(target_os = "linux")]
            blocking: remaining,
        })
    }
}

#[derive(Default)]
pub struct TokioCores {
    pub workers: Option<Vec<CoreId>>,
    // For blocking threads, we don't want to limit them to a specific number
    // and pin them to their own cores - they're supposed to run concurrently
    // with each other. However, `core_affinity` doesn't support affinity masks,
    // so we just use the Linux-specific API, since this is only a slight boost
    // and we don't care enough about performance on other platforms.
    #[cfg(target_os = "linux")]
    pub blocking: Option<nix::sched::CpuSet>,
}

impl TokioCores {
    /// Configures `builder` to pin its worker threads to specific cores.
    pub fn configure(self, builder: &mut tokio::runtime::Builder) {
        if let Some(cores) = self.workers {
            builder.worker_threads(cores.len());

            let cores_queue = Box::new(ArrayQueue::new(cores.len()));
            for core in cores {
                cores_queue.push(core).unwrap();
            }

            // `on_thread_start` gets called for both async worker threads and blocking threads,
            // but the first `worker_threads` threads that tokio spawns are worker threads,
            // so this ends up working fine
            builder.on_thread_start(move || {
                if let Some(core) = cores_queue.pop() {
                    core_affinity::set_for_current(core);
                } else {
                    #[cfg(target_os = "linux")]
                    if let Some(cpuset) = &self.blocking {
                        let this = nix::unistd::Pid::from_raw(0);
                        let _ = nix::sched::sched_setaffinity(this, cpuset);
                    }
                }
            });
        }
    }
}

#[derive(Default)]
pub struct RayonCores(Option<Vec<CoreId>>);

impl RayonCores {
    /// Configures a global rayon threadpool, pinning its threads to specific cores.
    ///
    /// All rayon threads will be run with `tokio_handle` enetered into.
    pub fn configure(self, tokio_handle: &tokio::runtime::Handle) {
        rayon_core::ThreadPoolBuilder::new()
            .thread_name(|_idx| "rayon-worker".to_string())
            .spawn_handler(thread_spawn_handler(tokio_handle))
            .num_threads(self.0.as_ref().map_or(0, |cores| cores.len()))
            .start_handler(move |i| {
                if let Some(cores) = &self.0 {
                    core_affinity::set_for_current(cores[i]);
                }
            })
            .build_global()
            .unwrap()
    }
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
