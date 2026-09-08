//! Local trusted tools run in a dedicated Unix process group. Cancellation
//! retains the workspace until that group is signalled and its leader reaped.
//! This is process cleanup, not containment of tools that deliberately escape
//! their process group.
use anyhow::Result;
use std::{ffi::OsString, future::Future, path::PathBuf, sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

pub struct Invocation {
    pub tool: PathBuf,
    pub label: &'static str,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
    pub workspace: Arc<TempDir>,
    pub timeout: Duration,
    pub cancel: CancellationToken,
}
pub struct Output {
    pub stdout: Vec<u8>,
}
pub trait Runner: Sync {
    fn run(&self, invocation: Invocation) -> impl Future<Output = Result<Output>> + Send;
}
pub struct LocalRunner;

impl Runner for LocalRunner {
    async fn run(&self, invocation: Invocation) -> Result<Output> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            local::run(invocation).await
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = invocation;
            anyhow::bail!(
                "local image tools currently require Linux or macOS; import an existing oci: directory instead"
            )
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod local {
    use super::*;
    use anyhow::{bail, ensure, Context};
    use rustix::process::{Pid, WaitIdOptions};
    use std::process::Stdio;
    use tokio::{
        io::AsyncReadExt,
        process::Command,
        sync::{oneshot, Semaphore},
    };

    static PROCESSES: std::sync::LazyLock<Arc<Semaphore>> = std::sync::LazyLock::new(|| Arc::new(Semaphore::new(2)));
    const MAX_OUTPUT: usize = 4 * 1024 * 1024;

    async fn read_bounded(mut reader: impl tokio::io::AsyncRead + Unpin) -> Result<Vec<u8>> {
        let mut bytes = vec![];
        (&mut reader)
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .await?;
        ensure!(bytes.len() <= MAX_OUTPUT, "builder diagnostic output exceeded 4 MiB");
        Ok(bytes)
    }

    // Keep the group leader unreaped until after killpg. Its waitable PID pins
    // the numeric process-group identity even when it exits before descendants.
    struct ProcessGroup(Option<Pid>);
    impl ProcessGroup {
        fn kill(&mut self) -> std::io::Result<()> {
            if let Some(pid) = self.0.take() {
                match rustix::process::kill_process_group(pid, rustix::process::Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => (),
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(())
        }
        async fn observe_exit(&mut self) -> Result<()> {
            let pid = self.0.context("local image tool ownership lost")?;
            loop {
                match rustix::process::waitid(
                    rustix::process::WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
                ) {
                    Ok(Some(_)) => return Ok(()),
                    Ok(None) | Err(rustix::io::Errno::INTR) => (),
                    Err(error) => {
                        // An external reaper invalidates numeric PID ownership.
                        // Never signal that group after this boundary.
                        if error == rustix::io::Errno::CHILD {
                            self.0 = None;
                        }
                        return Err(error).context("could not observe local image tool exit");
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    impl Drop for ProcessGroup {
        fn drop(&mut self) {
            let _ = self.kill();
        }
    }

    pub(super) async fn run(invocation: Invocation) -> Result<Output> {
        let permit = PROCESSES
            .clone()
            .try_acquire_owned()
            .context("two local image tools are already running")?;
        let (mut send, receive) = oneshot::channel();
        tokio::spawn(async move {
            let _permit = permit;
            let result = async {
                ensure!(!send.is_closed() && !invocation.cancel.is_cancelled(), "container build cancelled");
                let _workspace = invocation.workspace;
                let mut command = Command::new(&invocation.tool);
                command.args(&invocation.args).current_dir(&invocation.cwd).env_clear();
                // No implicit registry, Spacetime, proxy or builder credentials.
                for key in ["PATH", "SystemRoot", "TMPDIR", "TEMP", "TMP"] {
                    if let Some(value) = std::env::var_os(key) { command.env(key, value); }
                }
                command.envs(invocation.env).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
                let mut child = command.spawn().with_context(|| format!("could not run {}; install it or provide its executable path", invocation.tool.display()))?;
                let mut group = ProcessGroup(Some(Pid::from_raw(child.id().context("builder PID unavailable")? as i32).context("invalid builder PID")?));
                let stdout = child.stdout.take().context("builder stdout unavailable")?;
                let stderr = child.stderr.take().context("builder stderr unavailable")?;
                let outcome = {
                    let completion = async {
                        let (status, stdout, _) = tokio::try_join!(
                            async {
                                group.observe_exit().await?;
                                group.kill().context("failed to stop builder descendants")?;
                                Ok::<_, anyhow::Error>(child.wait().await?)
                            },
                            read_bounded(stdout),
                            read_bounded(stderr),
                        )?;
                        ensure!(status.success(), "{} failed ({status}); no prepared output was accepted", invocation.label);
                        Ok(Output { stdout })
                    };
                    tokio::select! {
                        biased;
                        _ = send.closed() => Err(anyhow::anyhow!("container build caller closed")),
                        _ = invocation.cancel.cancelled() => Err(anyhow::anyhow!("container build cancelled")),
                        _ = tokio::time::sleep(invocation.timeout) => Err(anyhow::anyhow!("{} exceeded its build deadline", invocation.label)),
                        result = completion => result,
                    }
                };
                // Disarmed before every reap, including completion above. Child
                // wait is cached if completion already reaped it.
                group.kill().context("failed to stop local image tool group")?;
                match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                    Ok(result) => { result.context("local image tool could not be reaped")?; },
                    Err(_) => {
                        // The owner retains the workspace and permit throughout
                        // delayed physical cleanup, even if the caller is gone.
                        child.wait().await.context("local image tool could not be reaped")?;
                        bail!("local image tool required delayed physical cleanup");
                    }
                }
                outcome
            }.await;
            let _ = send.send(result);
        });
        receive.await.context("local image tool owner stopped")?
    }
}
