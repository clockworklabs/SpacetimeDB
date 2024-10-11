use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use tokio::sync::broadcast;

use spacetimedb_paths::server::{ModuleLogPath, ReplicaDir};

pub struct DatabaseLogger {
    file: File,
    pub tx: broadcast::Sender<bytes::Bytes>,
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
    pub ts: chrono::DateTime<chrono::Utc>,
    pub target: Option<&'a str>,
    pub filename: Option<&'a str>,
    pub line_number: Option<u32>,
    pub message: &'a str,
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
    // fn log_dir_from(identity: Identity, _name: &str) -> PathBuf {
    //     let mut path = PathBuf::from(ROOT);
    //     path.push(Self::path_from_hash(identity));
    //     path
    // }

    // fn log_path_from(identity: Identity, name: &str) -> PathBuf {
    //     let mut path = Self::log_dir_from(identity, name);
    //     path.push(PathBuf::from_str(&format!("{}.log", name)).unwrap());
    //     path
    // }

    // fn path_from_hash(hash: Hash) -> PathBuf {
    //     let hex_address = hash.to_hex();
    //     let path = format!("{}/{}", &hex_address[0..2], &hex_address[2..]);
    //     PathBuf::from(path)
    // }

    pub fn filepath(replica_dir: ReplicaDir) -> ModuleLogPath {
        replica_dir.module_log(chrono::Utc::now().date_naive())
    }

    pub fn open(replica_dir: ReplicaDir) -> Self {
        Self::open_from_path(&Self::filepath(replica_dir))
    }

    pub fn open_from_path(path: &ModuleLogPath) -> Self {
        let file = path.open_file(File::options().create(true).append(true)).unwrap();
        let (tx, _) = broadcast::channel(64);
        Self { file, tx }
    }

    #[tracing::instrument(name = "DatabaseLogger::size", skip(self), err)]
    pub fn size(&self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    pub fn _delete(&mut self) {
        self.file.set_len(0).unwrap();
        self.file.seek(SeekFrom::End(0)).unwrap();
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
        (&self.file).write_all(buf.as_bytes()).unwrap();
        let _ = self.tx.send(buf.into());
    }

    pub async fn _read_all(filepath: &ModuleLogPath) -> String {
        tokio::fs::read_to_string(filepath).await.unwrap()
    }

    pub async fn read_latest(filepath: &ModuleLogPath, num_lines: Option<u32>) -> String {
        // TODO: Read backwards from the end of the file to only read in the latest lines
        let text = tokio::fs::read_to_string(filepath).await.expect("reading log file");

        let Some(num_lines) = num_lines else { return text };

        let off_from_end = text
            .split_inclusive('\n')
            .rev()
            .take(num_lines as usize)
            .map(|line| line.len())
            .sum::<usize>();

        text[text.len() - off_from_end..].to_owned()
    }

    pub fn system_logger(&self) -> &SystemLogger {
        // SAFETY: SystemLogger is repr(transparent) over DatabaseLogger
        unsafe { &*(self as *const DatabaseLogger as *const SystemLogger) }
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

    fn record(message: &str) -> Record {
        Record {
            ts: chrono::Utc::now(),
            target: None,
            filename: Some("spacetimedb"),
            line_number: None,
            message,
        }
    }
}
