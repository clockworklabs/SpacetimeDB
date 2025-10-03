use crossbeam_queue::ArrayQueue;
use itertools::Itertools;
use spacetimedb_paths::server::{ConfigToml, LogsDir};
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

pub use core_affinity::CoreId;

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
            if prev_time.is_none_or(|prev| modified > prev) {
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
    pin_threads_with_reservations(CoreReservations::default())
}

/// Like [`pin_threads`], but with a custom [`CoreReservations`].
#[must_use]
pub fn pin_threads_with_reservations(reservations: CoreReservations) -> Cores {
    Cores::get(reservations).unwrap_or_default()
}

/// The desired distribution of available cores to purposes.
///
/// Note that, in addition to `reserved`, [`Cores`] reserves two additional
/// cores for the operating system. That is, the denominator for fractions
/// given below is `num_cpus - reserved - 2`.
pub struct CoreReservations {
    /// Cores to run database instances on.
    ///
    /// Default: 1/8
    pub databases: f64,
    /// Cores to run tokio worker threads on.
    ///
    /// Default: 4/8
    pub tokio_workers: f64,
    /// Cores to run rayon threads on.
    ///
    /// Default: 1/8
    pub rayon: f64,
    /// Cores to reserve for IRQ handling.
    ///
    /// This will be the first `n` [`CoreId`]s in the list.
    /// Only make use of this if you're configuring the machine for IRQ pinning!
    ///
    /// Default: 2
    pub irq: usize,
    /// Extra reserved cores.
    ///
    /// If greater than zero, this many cores will be reserved _before_
    /// any of the other reservations are made (but after reserving the OS cores).
    ///
    /// Default: 0
    pub reserved: usize,
}

impl Default for CoreReservations {
    fn default() -> Self {
        Self {
            databases: 1.0 / 8.0,
            tokio_workers: 4.0 / 8.0,
            rayon: 1.0 / 8.0,
            irq: 2,
            reserved: 0,
        }
    }
}

impl CoreReservations {
    /// Apply this reservation to an arbitrary list of core ids.
    ///
    /// Returns the allocated cores in the order:
    ///
    /// - irq
    /// - reserved
    /// - databases
    /// - tokio_workers
    /// - rayon
    ///
    /// Left public for testing and debugging purposes.
    pub fn apply(&self, cores: &mut Vec<CoreId>) -> [Vec<CoreId>; 5] {
        let irq = cores.drain(..self.irq).collect_vec();
        let reserved = cores.drain(..self.reserved).collect_vec();

        let total = cores.len() as f64;
        let frac = |frac: f64| (total * frac).ceil() as usize;
        fn claim(cores: &mut Vec<CoreId>, n: usize) -> impl Iterator<Item = CoreId> + '_ {
            cores.drain(..n.min(cores.len()))
        }

        let databases = claim(cores, frac(self.databases)).collect_vec();
        let tokio_workers = claim(cores, frac(self.tokio_workers)).collect_vec();
        let rayon = claim(cores, frac(self.rayon)).collect_vec();

        [irq, reserved, databases, tokio_workers, rayon]
    }
}

/// A type holding cores divvied up into different sets.
///
/// Obtained from [`pin_threads()`].
#[derive(Default)]
pub struct Cores {
    /// The cores to run database instances on.
    pub databases: DatabaseCores,
    /// The cores to run tokio worker threads on.
    pub tokio: TokioCores,
    /// The cores to run rayon threads on.
    pub rayon: RayonCores,
    /// Extra cores if a [`CoreReservations`] with `reserved > 0` was used.
    ///
    /// If `Some`, the boxed array is non-empty.
    pub reserved: Option<Box<[CoreId]>>,
    /// Cores shared between tokio runtimes to schedule blocking tasks on.
    ///
    /// All remaining cores after [`CoreReservations`] have been made become
    /// blocking cores.
    ///
    /// See `Tokio.blocking` for more context.
    #[cfg(target_os = "linux")]
    pub blocking: Option<nix::sched::CpuSet>,
}

impl Cores {
    fn get(reservations: CoreReservations) -> Option<Self> {
        let mut cores = Self::get_core_ids()?;

        let [_irq, reserved, databases, tokio_workers, rayon] = reservations.apply(&mut cores);

        let databases = DatabaseCores(databases);
        let reserved = (!reserved.is_empty()).then(|| reserved.into());
        let rayon = RayonCores((!rayon.is_empty()).then_some(rayon));

        // see comment on `TokioCores.blocking`
        #[cfg(target_os = "linux")]
        let remaining = cores
            .into_iter()
            .try_fold(nix::sched::CpuSet::new(), |mut cpuset, core| {
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

    /// Get the cores of the local host, as reported by the operating system.
    ///
    /// Returns `None` if `num_cpus` is less than 8.
    /// If `Some` is returned, the `Vec` is non-empty.
    pub fn get_core_ids() -> Option<Vec<CoreId>> {
        let cores = core_affinity::get_core_ids()
            .filter(|cores| cores.len() >= 10)?
            .into_iter()
            .collect_vec();

        (!cores.is_empty()).then_some(cores)
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

#[derive(Default)]
pub struct DatabaseCores(Vec<CoreId>);

impl DatabaseCores {
    /// Construct a [`JobCores`] manager suitable for running database WASM code on.
    ///
    /// The `global_runtime` should be a [`tokio::runtime::Handle`] to the [`tokio::runtime::Runtime`]
    /// constructed from the [`TokioCores`] of this [`Cores`].
    ///
    /// ```rust
    /// # use spacetimedb::startup::pin_threads;
    /// let cores = pin_threads();
    /// let mut builder = tokio::runtime::Builder::new_multi_thread();
    /// cores.tokio.configure(&mut builder);
    /// let mut rt = builder.build().unwrap();
    /// let database_cores = cores.databases.make_database_runners(rt.handle());
    /// ```
    pub fn make_database_runners(self, global_runtime: &tokio::runtime::Handle) -> JobCores {
        JobCores::from_pinned_cores(self.0, global_runtime.clone())
    }
}
