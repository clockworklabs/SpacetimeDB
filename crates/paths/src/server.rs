use std::fmt::Write;
use std::{fs, io};

use crate::utils::{path_type, PathBufExt};
use chrono::NaiveDate;

path_type!(ServerDataDir: dir);

impl ServerDataDir {
    pub fn config_toml(&self) -> ConfigToml {
        ConfigToml(self.0.join("config.toml"))
    }

    pub fn logs(&self) -> LogsDir {
        LogsDir(self.0.join("logs"))
    }

    pub fn wasmtime_cache(&self) -> WasmtimeCacheDir {
        WasmtimeCacheDir(self.0.join("cache/wasmtime"))
    }

    pub fn metadata_toml(&self) -> MetadataTomlPath {
        MetadataTomlPath(self.0.join("metadata.toml"))
    }

    pub fn pid_file(&self) -> Result<PidFile, PidFileError> {
        use fs2::FileExt;
        use io::{Read, Write};
        self.create()?;
        let path = self.0.join("spacetime.pid");
        let mut file = fs::File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .read(true)
            .open(&path)?;
        match file.try_lock_exclusive() {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                let mut s = String::new();
                let pid = file.read_to_string(&mut s).ok().and_then(|_| s.trim().parse().ok());
                return Err(PidFileError::Exists { pid });
            }
            Err(e) => return Err(e.into()),
        }
        let mut pidfile = PidFile { file, path };
        pidfile.file.set_len(0)?;
        write!(pidfile.file, "{}", std::process::id())?;
        Ok(pidfile)
    }

    pub fn replica(&self, replica_id: u64) -> ReplicaDir {
        ReplicaDir(self.0.join("replicas").joined_int(replica_id))
    }
}

path_type!(ConfigToml: file);
path_type! {
    /// The directory in which server logs are to be stored.
    ///
    /// The files in this directory have the naming format `spacetime-{edition}.YYYY-MM-DD.log`.
    LogsDir: dir
}

impl LogsDir {
    // we can't be as strongly typed as we might like here, because `tracing_subscriber`'s
    // `RollingFileAppender` specifically takes the prefix and suffix of the filename, and
    // sticks the date in between them - so we have to expose those components of the
    // filename separately, rather than `fn logfile(&self, edition, date) -> LogFilePath`

    /// The prefix before the first `.` of a logfile name.
    pub fn filename_prefix(edition: &str) -> String {
        format!("spacetime-{edition}")
    }

    /// The file extension of a logfile name.
    pub fn filename_extension() -> String {
        "log".to_owned()
    }
}

path_type!(WasmtimeCacheDir: dir);

path_type!(MetadataTomlPath: file);

#[derive(thiserror::Error, Debug)]
pub enum PidFileError {
    #[error("error while taking database lock on spacetime.pid")]
    Io(#[from] io::Error),
    #[error("cannot take lock on database; spacetime.pid already exists (owned by pid {pid:?})")]
    Exists { pid: Option<u32> },
}

/// Removes file upon drop
pub struct PidFile {
    file: fs::File,
    path: std::path::PathBuf,
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

path_type!(ReplicaDir: dir);

impl ReplicaDir {
    /// `date` should be in UTC.
    pub fn module_log(self, date: NaiveDate) -> ModuleLogPath {
        let mut path = self.0.joined("module_logs/");
        write!(path.as_mut_os_string(), "{date}.date").unwrap();
        ModuleLogPath(path)
    }

    pub fn snapshots(&self) -> SnapshotsPath {
        SnapshotsPath(self.0.join("snapshots"))
    }

    pub fn commit_log(&self) -> CommitLogDir {
        CommitLogDir(self.0.join("clog"))
    }
}

path_type!(ModuleLogPath: file);

path_type!(SnapshotsPath: dir);

impl SnapshotsPath {
    pub fn snapshot_dir(&self, tx_offset: u64) -> SnapshotDirPath {
        let dir_name = format!("{tx_offset:0>20}.snapshot_dir");
        SnapshotDirPath(self.0.join(dir_name))
    }
}

path_type!(SnapshotDirPath: dir);

impl SnapshotDirPath {
    pub fn snapshot_file(&self, tx_offset: u64) -> SnapshotFilePath {
        let file_name = format!("{tx_offset:0>20}.snapshot_bsatn");
        SnapshotFilePath(self.0.join(file_name))
    }

    pub fn objects(&self) -> SnapshotObjectsPath {
        SnapshotObjectsPath(self.0.join("objects"))
    }

    pub fn rename_invalid(&self) -> io::Result<()> {
        let invalid_path = self.0.with_extension("invalid_snapshot");
        fs::rename(self, invalid_path)
    }
}

path_type!(SnapshotFilePath: file);
path_type!(SnapshotObjectsPath: dir);

path_type!(CommitLogDir: dir);

impl CommitLogDir {
    /// By convention, the file name of a segment consists of the minimum
    /// transaction offset contained in it, left-padded with zeroes to 20 digits,
    /// and the file extension `.stdb.log`.
    pub fn segment(&self, offset: u64) -> SegmentFile {
        let file_name = format!("{offset:0>20}.stdb.log");
        SegmentFile(self.0.join(file_name))
    }

    /// Returns the offset index file path based on the root path and offset
    pub fn index(&self, offset: u64) -> OffsetIndexFile {
        let file_name = format!("{offset:0>20}.stdb.ofs");
        OffsetIndexFile(self.0.join(file_name))
    }
}

path_type!(SegmentFile: file);
path_type!(OffsetIndexFile: file);
