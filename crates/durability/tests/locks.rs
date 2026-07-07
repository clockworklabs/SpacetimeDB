use std::error::Error;
use std::io;
use std::process::Command;

use spacetimedb_fs_utils::lockfile::advisory::LockedFile;

#[test]
fn lockedfile_blocks_same_process_second_lock() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("db.lock");
    let first = LockedFile::lock(&path)?;

    let second = LockedFile::lock(&path).unwrap_err();
    assert_eq!(second.source.kind(), io::ErrorKind::WouldBlock);
    println!(
        "same_process_second_lock_blocked=true path={} error_kind={:?}",
        path.display(),
        second.source.kind()
    );

    drop(first);
    Ok(())
}

#[test]
fn lockedfile_blocks_cross_process_second_lock() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("db.lock");
    let first = LockedFile::lock(&path)?;

    let script = r#"
import errno
import fcntl
import os
import sys

fd = os.open(sys.argv[1], os.O_RDWR | os.O_CREAT)
try:
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError as exc:
    print(f"blocked errno={exc.errno}")
    sys.exit(0)
else:
    print("acquired")
    sys.exit(2)
"#;
    let output = Command::new("python3").arg("-c").arg(script).arg(&path).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "child process unexpectedly acquired lock or failed: status={} stdout={stdout:?} stderr={stderr:?}",
        output.status
    );
    assert!(stdout.contains("blocked"), "unexpected child stdout: {stdout:?}");
    println!(
        "cross_process_second_lock_blocked=true path={} child_stdout={}",
        path.display(),
        stdout.trim()
    );

    drop(first);
    Ok(())
}
