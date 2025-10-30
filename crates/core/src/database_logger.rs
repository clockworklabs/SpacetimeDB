use chrono::{NaiveDate, Utc};
use parking_lot::Mutex;
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use tokio::sync::broadcast;

use spacetimedb_paths::server::{ModuleLogPath, ModuleLogsDir};

use crate::util::asyncify;

pub struct DatabaseLogger {
    inner: Mutex<DatabaseLoggerInner>,
    pub tx: broadcast::Sender<bytes::Bytes>,
}

struct DatabaseLoggerInner {
    file: File,
    date: NaiveDate,
    path: ModuleLogPath,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Panic,
}

impl From<u8> for LogLevel {
    fn from(level: u8) -> Self {
        match level {
            0 => LogLevel::Error,
            1 => LogLevel::Warn,
            2 => LogLevel::Info,
            3 => LogLevel::Debug,
            4 => LogLevel::Trace,
            101 => LogLevel::Panic,
            _ => LogLevel::Debug,
        }
    }
}

#[serde_with::skip_serializing_none]
#[serde_with::serde_as]
#[derive(serde::Serialize, Copy, Clone)]
pub struct Record<'a> {
    #[serde_as(as = "serde_with::TimestampMicroSeconds")]
    pub ts: chrono::DateTime<Utc>,
    /// Target of the log call (usually source namespace or `mod`).
    ///
    /// Provided by the WASM guest as an argument to the `console_log` host function.
    ///
    /// The special sentinel value [`Record::SENTINEL_INJECTED_TARGET`]` denotes logs injected by the [`SystemLogger`].
    pub target: Option<&'a str>,
    /// Filename of the source location of the log call.
    ///
    /// Provided by the WASM guest as an argument to the `console_log` host function.
    ///
    /// The special sentinel value [`Record::SENTINEL_INJECTED_FILENAME`]` denotes logs injected by the [`SystemLogger`].
    pub filename: Option<&'a str>,
    pub line_number: Option<u32>,
    /// Which exported function (i.e. reducer) was being called when this message was produced.
    ///
    /// Unlike `target`, `filename` and `line_number`, this is not provided by the WASM guest.
    /// Instead, the `WasmInstanceEnv` remembers what function call is in progress and adds it to the record.
    ///
    /// The special sentinel value [`Record::SENTINEL_INJECTED_FUNCTION`] denotes logs injected by the [`SystemLogger`].
    pub function: Option<&'a str>,
    pub message: &'a str,
}

impl<'a> Record<'a> {
    pub const SENTINEL_INJECTED_FUNCTION: Option<&'static str> = Some("__spacetimedb__");
    pub const SENTINEL_INJECTED_TARGET: Option<&'static str> = Some("__spacetimedb__");
    pub const SENTINEL_INJECTED_FILENAME: Option<&'static str> = Some("__spacetimedb__");

    /// Create a log `Record` for a system message, not attributed to any reducer or user filename.
    ///
    /// The resulting `Record` will draw from [`chrono::Utc::now`] for its timestamp,
    /// have `line_number: None`,
    /// and will use [`Self::SENTINEL_INJECTED_FILENAME`], [`Self::SENTINEL_INJECTED_FUNCTION`]
    /// and [`Self::SENTINEL_INJECTED_TARGET`].
    pub fn injected(message: &'a str) -> Self {
        Record {
            ts: chrono::Utc::now(),
            target: Self::SENTINEL_INJECTED_TARGET,
            filename: Self::SENTINEL_INJECTED_FILENAME,
            line_number: None,
            function: Self::SENTINEL_INJECTED_FUNCTION,
            message,
        }
    }
}

pub trait BacktraceProvider {
    fn capture(&self) -> Box<dyn ModuleBacktrace>;
}

impl BacktraceProvider for () {
    fn capture(&self) -> Box<dyn ModuleBacktrace> {
        Box::new(())
    }
}

pub trait ModuleBacktrace {
    fn frames(&self) -> Vec<BacktraceFrame<'_>>;
}

impl ModuleBacktrace for () {
    fn frames(&self) -> Vec<BacktraceFrame<'_>> {
        vec![]
    }
}

#[serde_with::skip_serializing_none]
#[serde_with::serde_as]
#[derive(serde::Serialize)]
pub struct BacktraceFrame<'a> {
    #[serde_as(as = "Option<DemangleSymbol>")]
    pub module_name: Option<&'a str>,
    #[serde_as(as = "Option<DemangleSymbol>")]
    pub func_name: Option<&'a str>,
}

struct DemangleSymbol;
impl serde_with::SerializeAs<&str> for DemangleSymbol {
    fn serialize_as<S>(source: &&str, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Ok(sym) = rustc_demangle::try_demangle(source) {
            serializer.serialize_str(&sym.to_string())
        } else {
            serializer.serialize_str(source)
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(serde::Serialize)]
#[serde(tag = "level")]
enum LogEvent<'a> {
    Error(Record<'a>),
    Warn(Record<'a>),
    Info(Record<'a>),
    Debug(Record<'a>),
    Trace(Record<'a>),
    Panic {
        #[serde(flatten)]
        record: Record<'a>,
        trace: &'a [BacktraceFrame<'a>],
    },
}

impl DatabaseLoggerInner {
    fn open(path: ModuleLogPath) -> io::Result<Self> {
        let date = path.date();
        let file = path.open_file(File::options().create(true).append(true))?;
        Ok(Self { file, date, path })
    }
}

impl DatabaseLogger {
    pub fn open_today(logs_dir: ModuleLogsDir) -> Self {
        Self::open(logs_dir.today())
    }

    pub fn open(path: ModuleLogPath) -> Self {
        let inner = Mutex::new(DatabaseLoggerInner::open(path).unwrap());
        let (tx, _) = broadcast::channel(64);
        Self { inner, tx }
    }

    #[tracing::instrument(level = "trace", name = "DatabaseLogger::size", skip(self), err)]
    pub fn size(&self) -> io::Result<u64> {
        Ok(self.inner.lock().file.metadata()?.len())
    }

    pub fn write(&self, level: LogLevel, &record: &Record<'_>, bt: &dyn BacktraceProvider) {
        let (trace, frames);
        let event = match level {
            LogLevel::Error => LogEvent::Error(record),
            LogLevel::Warn => LogEvent::Warn(record),
            LogLevel::Info => LogEvent::Info(record),
            LogLevel::Debug => LogEvent::Debug(record),
            LogLevel::Trace => LogEvent::Trace(record),
            LogLevel::Panic => {
                trace = bt.capture();
                frames = trace.frames();
                LogEvent::Panic { record, trace: &frames }
            }
        };
        let mut buf = serde_json::to_string(&event).unwrap();
        buf.push('\n');
        let mut inner = self.inner.lock();
        let record_date = record.ts.date_naive();
        if record_date > inner.date {
            let new_path = inner.path.with_date(record_date);
            *inner = DatabaseLoggerInner::open(new_path).unwrap();
        }
        inner.file.write_all(buf.as_bytes()).unwrap();
        let _ = self.tx.send(buf.into());
    }

    pub async fn read_latest(logs_dir: ModuleLogsDir, num_lines: Option<u32>) -> String {
        // TODO: do we want to logs from across multiple files?

        let Some(num_lines) = num_lines else {
            let path = logs_dir.today();
            // look for the most recent logfile.
            match tokio::fs::read_to_string(&path).await {
                Ok(contents) => return contents,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => panic!("couldn't read log file: {e}"),
            }
            // if there's none for today, read the directory and
            let logs_dir = path.popped();
            return asyncify(move || match logs_dir.most_recent()? {
                Some(newest_log_file) => std::fs::read_to_string(newest_log_file),
                None => Ok(String::new()),
            })
            .await
            .expect("couldn't read log file");
        };

        if num_lines == 0 {
            return String::new();
        }

        asyncify(move || read_latest_lines(logs_dir, num_lines))
            .await
            .expect("couldn't read log file")
    }

    pub fn system_logger(&self) -> &SystemLogger {
        // SAFETY: SystemLogger is repr(transparent) over DatabaseLogger
        unsafe { &*(self as *const DatabaseLogger as *const SystemLogger) }
    }
}

fn read_latest_lines(logs_dir: ModuleLogsDir, num_lines: u32) -> io::Result<String> {
    use std::fs::File;
    let path = logs_dir.today();
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let Some(path) = path.popped().most_recent()? else {
                return Ok(String::new());
            };
            File::open(path)?
        }
        Err(e) => return Err(e),
    };
    let mut lines_read: u32 = 0;
    // rough guess of an appropriate size for a chunk that could get all the lines in one,
    // assuming a line is roughly 150 bytes long, but capping our buffer size at 64KiB
    let chunk_size = std::cmp::min((num_lines as u64 * 150).next_power_of_two(), 0x10_000);
    let mut buf = vec![0; chunk_size as usize];
    // the file should end in a newline, so we skip that one character
    let mut pos = file.seek(io::SeekFrom::End(0))?.saturating_sub(1) as usize;
    'outer: while pos > 0 {
        let (new_pos, buf) = match pos.checked_sub(buf.len()) {
            Some(pos) => (pos, &mut buf[..]),
            None => (0, &mut buf[..pos]),
        };
        pos = new_pos;
        read_exact_at(&file, buf, pos as u64)?;
        for lf_pos in memchr::Memchr::new(b'\n', buf).rev() {
            lines_read += 1;
            if lines_read >= num_lines {
                pos += lf_pos + 1;
                break 'outer;
            }
        }
    }
    file.seek(io::SeekFrom::Start(pos as u64))?;
    buf.clear();
    let mut buf = String::from_utf8(buf).unwrap();
    file.read_to_string(&mut buf)?;
    Ok(buf)
}

fn read_exact_at(file: &std::fs::File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset)
    }
    #[cfg(not(unix))]
    {
        (&*file).seek(io::SeekFrom::Start(offset))?;
        (&*file).read_exact(buf)
    }
}

/// Somewhat ad-hoc wrapper around [`DatabaseLogger`] which allows to inject
/// "system messages" into the user-retrievable database / module log
#[repr(transparent)]
pub struct SystemLogger {
    inner: DatabaseLogger,
}

impl SystemLogger {
    pub fn info(&self, msg: &str) {
        self.inner.write(LogLevel::Info, &Self::record(msg), &())
    }

    pub fn warn(&self, msg: &str) {
        self.inner.write(LogLevel::Warn, &Self::record(msg), &())
    }

    pub fn error(&self, msg: &str) {
        self.inner.write(LogLevel::Error, &Self::record(msg), &())
    }

    fn record(message: &str) -> Record<'_> {
        Record::injected(message)
    }
}
