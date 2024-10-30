use std::fmt::Write;
use std::{fs, io};

use crate::utils::{path_type, PathBufExt};
use chrono::NaiveDate;

path_type! {
    /// The data-dir, where all database data is stored for a spacetime server process.
    ServerDataDir: dir
}

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
        pidfile.file.flush()?;
        Ok(pidfile)
    }

    pub fn replica(&self, replica_id: u64) -> ReplicaDir {
        ReplicaDir(self.0.join("replicas").joined_int(replica_id))
    }
}

path_type! {
    /// The `config.toml` file, where server configuration is stored.
    ConfigToml: file
}

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

path_type! {
    /// The directory we give to wasmtime to cache its compiled artifacts in.
    WasmtimeCacheDir: dir
}

path_type! {
    /// The `metadata.toml` file, where metadata about the server that owns this data-dir
    /// is stored. Machine-writable only.
    MetadataTomlPath: file
}

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

path_type! {
    /// A replica directory, where all the data for a module's database is stored.
    /// `{data-dir}/replicas/$replica_id/`
    ReplicaDir: dir
}

impl ReplicaDir {
    /// `date` should be in UTC.
    pub fn module_log(self, date: NaiveDate) -> ModuleLogPath {
        let mut path = self.0.joined("module_logs/");
        write!(path.as_mut_os_string(), "{date}.log").unwrap();
        ModuleLogPath(path)
    }

    pub fn snapshots(&self) -> SnapshotsPath {
        SnapshotsPath(self.0.join("snapshots"))
    }

    pub fn commit_log(&self) -> CommitLogDir {
        CommitLogDir(self.0.join("clog"))
    }
}

path_type! {
    /// A module log from a specific date.
    ModuleLogPath: file
}

path_type! {
    /// The snapshots directory. `{data-dir}/replica/$replica_id/snapshots`
    SnapshotsPath: dir
}

impl SnapshotsPath {
    pub fn snapshot_dir(&self, tx_offset: u64) -> SnapshotDirPath {
        let dir_name = format!("{tx_offset:0>20}.snapshot_dir");
        SnapshotDirPath(self.0.join(dir_name))
    }
}

path_type! {
    /// A snapshot directory. `{data-dir}/replica/$replica_id/snapshots/$tx_offset.snapshot_dir`
    SnapshotDirPath: dir
}

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

path_type! {
    /// A snapshot file.
    /// `{data-dir}/replica/$replica_id/snapshots/$tx_offset.snapshot_dir/$tx_offset.snapshot_bsatn`
    SnapshotFilePath: file
}
path_type! {
    /// The objects directory for a snapshot.
    /// `{data-dir}/replica/$replica_id/snapshots/$tx_offset.snapshot_dir/objects`
    SnapshotObjectsPath: dir
}

path_type! {
    /// The commit log directory. `{data-dir}/replica/$replica_id/clog`
    CommitLogDir: dir
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_pid_file_is_written() -> Result<()> {
        let tempdir = TempDir::new()?;
        let sdd = ServerDataDir(tempdir.path().to_path_buf());

        let lock = sdd.pid_file()?;

        // Make sure we wrote the pid file.
        let pidstring = fs::read_to_string(lock.path.clone())?;
        let _pid = pidstring.trim().parse::<u32>()?;

        Ok(())
    }

    #[test]
    fn test_pid_is_exclusive() -> Result<()> {
        let tempdir = TempDir::new()?;
        let sdd = ServerDataDir(tempdir.path().to_path_buf());

        let lock = sdd.pid_file()?;

        // Make sure we wrote the pid file.
        let pidstring = fs::read_to_string(lock.path.clone())?;
        let _pid = pidstring.trim().parse::<u32>()?;

        let attempt = sdd.pid_file();
        assert!(attempt.is_err());

        drop(lock);
        // Make sure it can be acquired now.
        sdd.pid_file()?;
        Ok(())
    }
}
