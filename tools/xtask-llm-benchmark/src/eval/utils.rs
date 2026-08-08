use std::env;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub fn derive_cat_task_from_file(src: &str) -> (String, String) {
    let p = std::path::Path::new(src);
    let task = p
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let cat = p
        .parent()
        .and_then(|d| d.parent())
        .and_then(|d| d.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    (cat, task)
}

pub(crate) fn spacetime_command() -> Command {
    if let Some(executable) = env::var_os("LLM_BENCH_SPACETIME_BIN") {
        return Command::new(executable);
    }

    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask-llm-benchmark is under tools/xtask-llm-benchmark");
    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let target = if target.is_absolute() {
        target
    } else {
        workspace.join(target)
    };
    let executable = if cfg!(windows) {
        "spacetimedb-cli.exe"
    } else {
        "spacetimedb-cli"
    };
    for profile in ["release", "debug"] {
        let candidate = target.join(profile).join(executable);
        if candidate.is_file() {
            return Command::new(candidate);
        }
    }

    Command::new("spacetime")
}

pub fn sql_exec(db: &str, query: &str, host: Option<&str>) -> Result<(), String> {
    sql_exec_with_timeout(db, query, host, Duration::from_secs(30))
}

pub(crate) fn sql_exec_with_timeout(
    db: &str,
    query: &str,
    host: Option<&str>,
    timeout: Duration,
) -> Result<(), String> {
    let mut cmd = spacetime_command();
    cmd.arg("sql").arg(db).arg(query);
    if let Some(h) = host {
        cmd.arg("--server").arg(h);
    }
    let (code, _, stderr) = run_with_timeout(cmd, Path::new("."), timeout)
        .map_err(|e| format!("spacetime sql failed or timed out: {e}"))?;
    if code != 0 {
        return Err(format!("spacetime sql failed:\n{}", String::from_utf8_lossy(&stderr)));
    }
    Ok(())
}

pub(crate) fn run_with_timeout(mut cmd: Command, cwd: &Path, timeout: Duration) -> io::Result<(i32, Vec<u8>, Vec<u8>)> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("stdout was configured as piped");
    let mut stderr = child.stderr.take().expect("stderr was configured as piped");
    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output)?;
        Ok::<_, io::Error>(output)
    });
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stderr.read_to_end(&mut output)?;
        Ok::<_, io::Error>(output)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            let termination_error = kill_process_tree(&mut child).err();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            let message = termination_error
                .map(|error| format!("process timeout; failed to terminate process tree: {error}"))
                .unwrap_or_else(|| "process timeout".to_string());
            return Err(io::Error::new(io::ErrorKind::TimedOut, message));
        }
        thread::sleep(Duration::from_millis(30));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| io::Error::other("stdout reader thread panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| io::Error::other("stderr reader thread panicked"))??;
    Ok((status.code().unwrap_or(-1), stdout, stderr))
}

fn kill_process_tree(child: &mut Child) -> io::Result<()> {
    #[cfg(windows)]
    let killed = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &child.id().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());

    #[cfg(unix)]
    let killed = Command::new("kill")
        .args(["-KILL", &format!("-{}", child.id())])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());

    if killed {
        Ok(())
    } else {
        child.kill()
    }
}

pub fn normalize(s: &str, collapse_ws: bool) -> String {
    let t = s.trim();
    if collapse_ws {
        t.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn process_timeout_kills_a_long_running_command() {
        let _guard = PROCESS_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        #[cfg(windows)]
        let command = {
            let mut command = Command::new("powershell");
            command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 5"]);
            command
        };
        #[cfg(not(windows))]
        let command = {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 5"]);
            command
        };

        let started = Instant::now();
        let error = run_with_timeout(command, Path::new("."), Duration::from_millis(100)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn process_output_is_drained_while_the_command_runs() {
        let _guard = PROCESS_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        #[cfg(windows)]
        let command = {
            let mut command = Command::new("powershell");
            command.args([
                "-NoProfile",
                "-Command",
                "$chunk = 'x' * 4096; 1..16 | ForEach-Object { [Console]::Out.Write($chunk); [Console]::Error.Write($chunk) }",
            ]);
            command
        };
        #[cfg(not(windows))]
        let command = {
            let mut command = Command::new("sh");
            command.args([
                "-c",
                "dd if=/dev/zero bs=4096 count=256 2>/dev/null; dd if=/dev/zero bs=4096 count=256 1>&2 2>/dev/null",
            ]);
            command
        };

        #[cfg(windows)]
        let expected_len = 4096 * 16;
        #[cfg(not(windows))]
        let expected_len = 4096 * 256;

        let (code, stdout, stderr) = run_with_timeout(command, Path::new("."), Duration::from_secs(60))
            .expect("large output should not deadlock");

        assert_eq!(code, 0);
        assert_eq!(stdout.len(), expected_len);
        assert_eq!(stderr.len(), expected_len);
    }
}
