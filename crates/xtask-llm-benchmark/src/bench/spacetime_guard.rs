use crate::bench::utils::server_name;
use anyhow::{bail, Context, Result};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct SpacetimeGuard {
    started_by_us: bool,
}

fn ping() -> Result<()> {
    let name = server_name();
    let out = Command::new("spacetime")
        .args(["server", "ping"])
        .arg(&name)
        .output()
        .context("spacetime server ping")?;
    if out.status.success() {
        Ok(())
    } else {
        bail!("{}", String::from_utf8_lossy(&out.stderr))
    }
}

impl SpacetimeGuard {
    pub fn acquire() -> Result<Self> {
        if ping().is_ok() {
            return Ok(Self { started_by_us: false });
        }
        Command::new("spacetime")
            .arg("start")
            .arg("--in-memory")
            .spawn()
            .context("spacetime start")?;
        let t0 = Instant::now();
        loop {
            if ping().is_ok() {
                break;
            }
            if t0.elapsed() > Duration::from_secs(30) {
                bail!("spacetime did not become ready");
            }
            sleep(Duration::from_millis(250));
        }
        Ok(Self { started_by_us: true })
    }
}

impl Drop for SpacetimeGuard {
    fn drop(&mut self) {
        // no stop command available; leave running if it was already up
        let _ = self.started_by_us;
    }
}
