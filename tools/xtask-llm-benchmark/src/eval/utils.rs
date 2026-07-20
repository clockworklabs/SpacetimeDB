use std::env;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
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
    let executable = env::var_os("LLM_BENCH_SPACETIME_BIN").unwrap_or_else(|| "spacetime".into());
    Command::new(executable)
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
    let mut child = cmd
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let out = child.wait_with_output()?;
            return Ok((status.code().unwrap_or(-1), out.stdout, out.stderr));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::TimedOut, "process timeout"));
        }
        thread::sleep(Duration::from_millis(30));
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

    #[test]
    fn process_timeout_kills_a_long_running_command() {
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
}
