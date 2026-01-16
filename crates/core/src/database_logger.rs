use bytes::Bytes;
use chrono::{NaiveDate, Utc};
use futures::stream::{self, BoxStream};
use futures::{Stream, StreamExt as _, TryStreamExt};
use pin_project_lite::pin_project;
use std::collections::VecDeque;
use std::fs::File;
use std::future;
use std::io::{self, Read, Seek, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::io::ReaderStream;

use spacetimedb_paths::server::{ModuleLogPath, ModuleLogsDir};

use crate::util::asyncify;

pub struct DatabaseLogger {
    cmd: mpsc::UnboundedSender<Cmd>,
}

#[derive(Debug, thiserror::Error)]
#[error("database logger panicked")]
pub struct LoggerPanicked;

impl<T> From<mpsc::error::SendError<T>> for LoggerPanicked {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Self
    }
}

impl From<oneshot::error::RecvError> for LoggerPanicked {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self
    }
}

pub type LogStream = BoxStream<'static, io::Result<Bytes>>;

/// Storage backend of a [DatabaseLogger].
trait Logger {
    /// Append the serialized log `record` at timestamp `ts` to this logger.
    fn append(&mut self, ts: chrono::DateTime<Utc>, record: Bytes);
    /// Calculate the size of this logger in bytes.
    fn size(&self) -> io::Result<u64>;
    /// Read up to `n` lines from the tail of this logger into memory.
    fn tail(&self, n: u32) -> io::Result<Bytes>;
    /// Stream up to `n` lines from the tail of this logger.
    /// If `n` is `None`, stream all the contained lines.
    fn tail_stream(&self, n: Option<u32>) -> LogStream;
    /// Sync data to disk (or alternative backing storage).
    fn sync_data(&self) -> io::Result<()>;
}

/// [Logger] that stores log records in a file.
///
/// The file is rotated daily upon calling [Logger::append].
struct FileLogger {
    file: File,
    date: NaiveDate,
    path: ModuleLogPath,
}

impl FileLogger {
    pub fn open(path: ModuleLogPath) -> io::Result<Self> {
        let date = path.date();
        let file = path.open_file(File::options().create(true).append(true))?;
        Ok(Self { file, date, path })
    }

    fn maybe_rotate(&mut self, record_date: NaiveDate) {
        if record_date > self.date {
            let new_path = self.path.with_date(record_date);
            *self = Self::open(new_path).unwrap();
        }
    }
}

impl Logger for FileLogger {
    fn append(&mut self, ts: chrono::DateTime<Utc>, record: Bytes) {
        self.maybe_rotate(ts.date_naive());
        self.file.write_all(&record).unwrap();
    }

    fn size(&self) -> io::Result<u64> {
        self.file.metadata().map(|stat| stat.len())
    }

    fn tail(&self, n: u32) -> io::Result<Bytes> {
        let mut file = File::open(&self.path)?;
        read_lines(&mut file, n).map(Into::into)
    }

    fn tail_stream(&self, n: Option<u32>) -> LogStream {
        stream::once(asyncify({
            let path = self.path.clone();
            move || {
                let mut file = File::open(path)?;
                if let Some(n) = n {
                    let mut buf = seek_buffer(n);
                    seek_to(&mut file, &mut buf, n)?;
                }

                Ok::<_, io::Error>(tokio::fs::File::from_std(file))
            }
        }))
        .map_ok(ReaderStream::new)
        .try_flatten()
        .boxed()
    }

    fn sync_data(&self) -> io::Result<()> {
        self.file.sync_data()
    }
}

/// [Logger] that stores log records in memory.
struct MemoryLogger {
    log: VecDeque<Bytes>,
    size: u64,
    max_size: u64,
}

impl MemoryLogger {
    pub fn new(max_size: u64) -> Self {
        Self {
            log: <_>::default(),
            size: 0,
            max_size,
        }
    }

    fn compact(&mut self) {
        while self.size > self.max_size {
            let Some(evicted) = self.log.pop_front() else {
                break;
            };
            self.size -= evicted.len() as u64;
        }
    }
}

impl Logger for MemoryLogger {
    fn append(&mut self, _: chrono::DateTime<Utc>, record: Bytes) {
        self.size += record.len() as u64;
        self.log.push_back(record);
        self.compact();
    }

    fn size(&self) -> io::Result<u64> {
        Ok(self.size)
    }

    fn tail(&self, n: u32) -> io::Result<Bytes> {
        let total = self.log.len();
        let start = total.saturating_sub(n as _);
        Ok(self.log.range(start..).flatten().copied().collect())
    }

    fn tail_stream(&self, n: Option<u32>) -> LogStream {
        let total = self.log.len();
        let start = total.saturating_sub(n.map(|x| x as usize).unwrap_or(total));
        stream::iter(self.log.range(start..).cloned().collect::<Vec<_>>())
            .map(Ok)
            .boxed()
    }

    fn sync_data(&self) -> io::Result<()> {
        Ok(())
    }
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
#[cfg_attr(test, derive(serde::Deserialize, Debug))]
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

impl DatabaseLogger {
    pub fn in_memory(max_size: u64) -> Self {
        let logger = MemoryLogger::new(max_size);
        Self::with_logger(logger)
    }

    pub fn open_today(logs_dir: ModuleLogsDir) -> Self {
        Self::open_file(logs_dir.today())
    }

    fn open_file(path: ModuleLogPath) -> Self {
        let logger = FileLogger::open(path).unwrap();
        Self::with_logger(logger)
    }

    fn with_logger(logger: impl Logger + Send + 'static) -> Self {
        let (broadcast, _) = broadcast::channel(64);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let worker = DatabaseLoggerWorker::new(logger, broadcast);
        tokio::spawn(worker.run(cmd_rx));

        Self { cmd: cmd_tx }
    }

    /// Determine the storage size of this logger.
    ///
    /// If the logger is [Self::in_memory], returns the resident size
    /// (of the serialized log records excl. overhead).
    ///
    /// If the logger is backed by disk storage, returns the size of the most
    /// recent log file.
    #[tracing::instrument(level = "trace", name = "DatabaseLogger::size", skip(self), err)]
    pub fn size(&self) -> io::Result<u64> {
        let (tx, rx) = oneshot::channel();
        fn panicked(_: impl std::error::Error) -> io::Error {
            io::Error::other("log worker panicked")
        }
        self.cmd.send(Cmd::GetSize { reply: tx }).map_err(panicked)?;
        rx.blocking_recv().map_err(panicked)?
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
        // TODO(perf): Reuse serialization buffer.
        let mut buf = serde_json::to_string(&event).unwrap();
        buf.push('\n');
        let buf = Bytes::from(buf);
        self.cmd
            .send(Cmd::Append {
                ts: record.ts,
                record: buf,
            })
            .expect("log worker panicked");
    }

    /// Stream the contents of this logger.
    ///
    /// If `n` is `Some`, only yield up to the last `n` lines in the log.
    /// If `follow` is `true`, the stream waits for new records to be appended
    /// to the log (via [Self::write]) and yields them as they become available.
    pub async fn tail(&self, n: Option<u32>, follow: bool) -> Result<LogStream, LoggerPanicked> {
        let (tx, rx) = oneshot::channel();
        self.cmd.send(Cmd::Tail { n, follow, reply: tx })?;
        Ok(rx.await?)
    }

    /// Read the most recent logs in `logs_dir`, up to `num_lines`.
    ///
    /// Note that this only reads from the most recent log file, even if it
    /// contains less than `num_lines` lines.
    ///
    /// If no log file exists on disk, the stream will be empty.
    pub fn read_latest_on_disk(logs_dir: ModuleLogsDir, num_lines: Option<u32>) -> LogStream {
        stream::once(asyncify(move || {
            let Some(mut file) = Self::open_most_recent(logs_dir)? else {
                return Ok(None);
            };
            if let Some(n) = num_lines {
                let mut buf = seek_buffer(n);
                seek_to(&mut file, &mut buf, n)?;
            }

            Ok::<_, io::Error>(Some(file))
        }))
        .map_ok(into_file_stream)
        .try_flatten()
        .boxed()
    }

    /// Open the most recent log file found in `logs_dir`, or `None` if none exists.
    fn open_most_recent(logs_dir: ModuleLogsDir) -> io::Result<Option<File>> {
        let path = logs_dir.today();
        match open_file(&path)? {
            Some(file) => Ok(Some(file)),
            None => {
                let logs_dir = path.popped();
                // `most_recent` errors if the directory doesn't exist.
                if !logs_dir.0.try_exists()? {
                    return Ok(None);
                }
                let Some(path) = logs_dir.most_recent()? else {
                    return Ok(None);
                };
                open_file(&path)
            }
        }
    }

    pub fn system_logger(&self) -> &SystemLogger {
        // SAFETY: SystemLogger is repr(transparent) over DatabaseLogger
        unsafe { &*(self as *const DatabaseLogger as *const SystemLogger) }
    }
}

enum Cmd {
    Append {
        ts: chrono::DateTime<Utc>,
        record: Bytes,
    },
    GetSize {
        reply: oneshot::Sender<io::Result<u64>>,
    },
    Tail {
        n: Option<u32>,
        follow: bool,
        reply: oneshot::Sender<LogStream>,
    },
}

struct DatabaseLoggerWorker<T> {
    logger: Arc<tokio::sync::Mutex<T>>,
    broadcast: broadcast::Sender<Bytes>,
}

impl<T: Logger + Send + 'static> DatabaseLoggerWorker<T> {
    fn new(logger: T, broadcast: broadcast::Sender<Bytes>) -> Self {
        let logger = Arc::new(tokio::sync::Mutex::new(logger));
        Self { logger, broadcast }
    }

    async fn run(self, mut cmd: mpsc::UnboundedReceiver<Cmd>) {
        while let Some(cmd) = cmd.recv().await {
            match cmd {
                Cmd::Append { ts, record } => self.append(ts, record).await,
                Cmd::GetSize { reply } => {
                    let size = self.size().await;
                    let _ = reply.send(size);
                }
                Cmd::Tail { n, follow, reply } => {
                    let logs = self.tail(n, follow).await;
                    let _ = reply.send(logs);
                }
            }
        }
    }

    async fn append(&self, ts: chrono::DateTime<Utc>, record: Bytes) {
        asyncify({
            let logger = self.logger.clone();
            let record = record.clone();
            move || logger.blocking_lock().append(ts, record)
        })
        .await;
        let _ = self.broadcast.send(record);
    }

    async fn size(&self) -> io::Result<u64> {
        let logger = self.logger.clone();
        asyncify(move || logger.blocking_lock().size()).await
    }

    async fn tail(&self, n: Option<u32>, follow: bool) -> LogStream {
        // If following isn't requested, we can stream the data.
        if !follow {
            return self.logger.lock().await.tail_stream(n);
        }
        match n {
            // If we don't need to access the disk,
            // locking and spawning can be avoided.
            None | Some(0) => self.subscribe().map(Ok).boxed(),

            // Otherwise, we need to hold the lock to prevent writes
            // while we gather a snapshot of the persistent tail.
            Some(n) => {
                // Cap reading the tail into memory at a few hundred KiB.
                let n = n.min(2500);
                let (tail, more) = {
                    let inner = self.logger.clone().lock_owned().await;
                    let more = self.subscribe();
                    asyncify(move || {
                        inner.sync_data().expect("error syncing data to disk");
                        (inner.tail(n), more)
                    })
                }
                .await;

                stream::once(future::ready(tail)).chain(more.map(Ok)).boxed()
            }
        }
    }

    fn subscribe(&self) -> impl Stream<Item = Bytes> {
        BroadcastStream::new(self.broadcast.subscribe()).filter_map(move |x| {
            future::ready(match x {
                Ok(chunk) => Some(chunk),
                Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                    log::trace!("skipped {skipped} lines in module log");
                    None
                }
            })
        })
    }
}

fn read_lines(file: &mut File, num_lines: u32) -> io::Result<Vec<u8>> {
    let mut buf = seek_buffer(num_lines);
    seek_to(file, &mut buf, num_lines)?;
    buf.clear();

    file.read_to_end(&mut buf)?;

    Ok(buf)
}

/// Allocate a buffer to use with [seek_to].
///
/// We assume a log line is typically around 150 bytes long, and allocate space
/// to fit `num_lines` in one read. The max size of the buffer is 64KiB.
fn seek_buffer(num_lines: u32) -> Vec<u8> {
    let chunk_size = std::cmp::min((num_lines as u64 * 150).next_power_of_two(), 0x10_000);
    vec![0; chunk_size as usize]
}

/// Set `file`'s position such that reading to the end will yield `num_lines`.
///
/// If `file` contains less than `num_lines`, the position is set to the start.
///
/// The function repeatedly fills `buf` from the end of the file, and counts the
/// number of LF characters in the buffer at each step until `num_lines` is
/// satisfied.
///
/// `buf` should be created via [seek_buffer] and is supplied by the caller in
/// order to allow reuse of the allocation.
fn seek_to(file: &mut File, buf: &mut [u8], num_lines: u32) -> io::Result<()> {
    let mut lines_read: u32 = 0;
    // the file should end in a newline, so we skip that one character
    let mut pos = file.seek(io::SeekFrom::End(0))?.saturating_sub(1) as usize;
    'outer: while pos > 0 {
        let (new_pos, buf) = match pos.checked_sub(buf.len()) {
            Some(pos) => (pos, &mut buf[..]),
            None => (0, &mut buf[..pos]),
        };
        pos = new_pos;
        read_exact_at(file, buf, pos as u64)?;
        for lf_pos in memchr::Memchr::new(b'\n', buf).rev() {
            lines_read += 1;
            if lines_read >= num_lines {
                pos += lf_pos + 1;
                break 'outer;
            }
        }
    }
    file.seek(io::SeekFrom::Start(pos as u64))?;

    Ok(())
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

/// Open the [File] at `path` for reading, or `None` if the file doesn't exist.
fn open_file(path: impl AsRef<Path>) -> io::Result<Option<File>> {
    File::open(path).map(Some).or_else(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(e)
        }
    })
}

/// Create a buffered [Stream] from a file.
///
/// If `file` is `None`, the stream is empty.
fn into_file_stream(file: impl Into<Option<File>>) -> impl Stream<Item = io::Result<Bytes>> {
    ReaderStream::new(BufReader::new(MaybeFile::new(file.into())))
}

pin_project! {
    #[project = MaybeFileProj]
    enum MaybeFile {
        File { #[pin] inner: tokio::fs::File },
        Empty,
    }
}

impl MaybeFile {
    pub fn new(file: Option<File>) -> Self {
        match file.map(tokio::fs::File::from_std) {
            Some(inner) => Self::File { inner },
            None => Self::Empty,
        }
    }
}

impl AsyncRead for MaybeFile {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeFileProj::File { inner } => inner.poll_read(cx, buf),
            MaybeFileProj::Empty => Poll::Ready(Ok(())),
        }
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

#[cfg(test)]
mod tests {
    use std::{ops::Range, sync::Arc};

    use bytes::BytesMut;
    use futures::TryStreamExt as _;

    use crate::util::asyncify;

    use super::{DatabaseLogger, LogLevel, Record};

    async fn write_logs(logger: Arc<DatabaseLogger>, r: Range<usize>) {
        asyncify(move || {
            for i in r {
                logger.write(
                    LogLevel::Info,
                    &Record {
                        ts: chrono::Utc::now(),
                        target: None,
                        filename: None,
                        line_number: None,
                        function: None,
                        message: &format!("log line {i}"),
                    },
                    &(),
                );
            }
        })
        .await
    }

    fn deserialize_logs<'a>(raw: &'a [u8]) -> Result<Vec<Record<'a>>, serde_json::Error> {
        serde_json::StreamDeserializer::new(serde_json::de::SliceRead::new(raw)).collect()
    }

    fn drop_logger(logger: Arc<DatabaseLogger>) {
        Arc::try_unwrap(logger)
            .map(drop)
            .map_err(drop)
            .expect("logger should be unique");
    }

    /// Test calling [DatabaseLogger::tail] with `Some(n)`.
    ///
    /// Like `tail -n`.
    async fn tail_n(logger: DatabaseLogger) {
        let logger = Arc::new(logger);

        write_logs(logger.clone(), 0..10).await;

        let a = logger
            .tail(Some(10), false)
            .await
            .unwrap()
            .try_collect::<BytesMut>()
            .await
            .unwrap();
        let b = logger
            .tail(None, false)
            .await
            .unwrap()
            .try_collect::<BytesMut>()
            .await
            .unwrap();
        assert_eq!(a, b);

        let c = logger
            .tail(Some(5), false)
            .await
            .unwrap()
            .try_collect::<BytesMut>()
            .await
            .unwrap();
        let json_logs = deserialize_logs(&c).unwrap();

        assert_eq!(json_logs.len(), 5);
        assert_eq!(json_logs[0].message, "log line 5");
        assert_eq!(json_logs[4].message, "log line 9");
    }

    /// Test calling [DatabaseLogger::tail] with
    /// `follow = true`.
    ///
    /// Like `tail -f`.
    async fn tail_f(logger: DatabaseLogger) {
        let logger = Arc::new(logger);

        let stream = logger.tail(None, true).await.unwrap().try_collect::<BytesMut>();
        write_logs(logger.clone(), 0..10).await;
        // Drop logger so stream terminates.
        drop_logger(logger);

        let raw_logs = stream.await.unwrap();
        let json_logs = deserialize_logs(&raw_logs).unwrap();

        assert_eq!(json_logs.len(), 10);
        assert_eq!(json_logs[0].message, "log line 0");
        assert_eq!(json_logs[9].message, "log line 9");
    }

    /// Test calling [DatabaseLogger::tail] with
    /// both `Some(n)` and `follow = true`.
    ///
    /// Like `tail -n N -f`.
    async fn tail_nf(logger: DatabaseLogger) {
        let logger = Arc::new(logger);

        write_logs(logger.clone(), 0..10).await;
        let stream = logger.tail(Some(5), true).await.unwrap().try_collect::<BytesMut>();
        write_logs(logger.clone(), 10..20).await;
        // Drop logger so stream terminates.
        drop_logger(logger);

        let raw_logs = stream.await.unwrap();
        let json_logs = deserialize_logs(&raw_logs).unwrap();

        assert_eq!(json_logs.len(), 15);
        assert_eq!(json_logs[0].message, "log line 5");
        assert_eq!(json_logs[14].message, "log line 19");
    }

    mod memory {
        use super::DatabaseLogger;

        #[tokio::test]
        async fn tail_n() {
            super::tail_n(DatabaseLogger::in_memory(1024)).await
        }

        #[tokio::test]
        async fn tail_f() {
            super::tail_f(DatabaseLogger::in_memory(1024)).await
        }

        #[tokio::test]
        async fn tail_nf() {
            super::tail_nf(DatabaseLogger::in_memory(1024)).await
        }
    }

    mod file {
        use std::future::Future;

        use spacetimedb_paths::{server::ModuleLogsDir, FromPathUnchecked};
        use tempfile::tempdir;

        use super::DatabaseLogger;

        #[tokio::test]
        async fn tail_n() {
            with_file_logger(super::tail_n).await
        }

        #[tokio::test]
        async fn tail_f() {
            with_file_logger(super::tail_f).await
        }

        #[tokio::test]
        async fn tail_nf() {
            with_file_logger(super::tail_nf).await
        }

        async fn with_file_logger<F, Fut>(f: F)
        where
            F: FnOnce(DatabaseLogger) -> Fut,
            Fut: Future<Output = ()>,
        {
            let tmp = tempdir().unwrap();
            let logs_dir = ModuleLogsDir::from_path_unchecked(tmp.path());
            let logger = DatabaseLogger::open_today(logs_dir);

            f(logger).await
        }
    }
}
