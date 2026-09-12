//! Run only from container_build_acceptance/acceptance.py, which owns and
//! verifies the Docker/BuildKit endpoint and all paths below. No saved defaults.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use anyhow::{ensure, Context, Result};
use spacetimedb_cli::container::{
    config::ContainerConfig,
    prepare_container,
    process::{Invocation, LocalRunner, Output, Runner},
    BuildTools,
};
use spacetimedb_lib::container::ImagePlatform;
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

struct ShortDeadline;
impl Runner for ShortDeadline {
    async fn run(&self, mut invocation: Invocation) -> Result<Output> {
        // Exercise the production local owner/kill/reap path without making
        // acceptance wait for the ordinary thirty-minute build deadline.
        invocation.timeout = Duration::from_secs(2);
        LocalRunner.run(invocation).await
    }
}

fn input(name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name).with_context(|| format!("explicit {name} is required"))?);
    ensure!(path.is_absolute(), "fixture input must be absolute");
    Ok(path)
}

#[tokio::test]
#[ignore = "requires the explicitly owned local BuildKit acceptance fixture"]
async fn actual_buildctl_deadline_reaps_before_workspace_release() -> Result<()> {
    let context = input("STDB_BUILDER_CONTEXT")?.canonicalize()?;
    let tool = input("STDB_BUILDER_BUILDCTL")?.canonicalize()?;
    let pid_file = input("STDB_BUILDER_PID_FILE")?;
    let workspace = input("STDB_BUILDER_WORKSPACE")?.canonicalize()?;
    let endpoint = std::env::var("STDB_BUILDER_SOCKET")?;
    ensure!(
        endpoint.starts_with("unix:///"),
        "explicit local BuildKit Unix socket is required"
    );
    let document: serde_json::Value = serde_json::from_slice(&std::fs::read(context.join("spacetime.json"))?)?;
    let configuration: ContainerConfig = serde_json::from_value(document["container"].clone())?;
    let result = prepare_container(
        &configuration,
        &context,
        ImagePlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
        },
        &BuildTools {
            buildctl: tool,
            buildkit_host: Some(endpoint),
            ..Default::default()
        },
        &workspace,
        &ShortDeadline,
        CancellationToken::new(),
    )
    .await;
    let error = result.err().context("long-running real build unexpectedly succeeded")?;
    ensure!(
        error.to_string().contains("build deadline"),
        "unexpected build failure: {error:#}"
    );
    let pid = std::fs::read_to_string(pid_file)?.trim().parse()?;
    let pid = rustix::process::Pid::from_raw(pid).context("invalid recorded builder PID")?;
    ensure!(
        rustix::process::test_kill_process(pid) == Err(rustix::io::Errno::SRCH),
        "real buildctl PID still exists after the deadline returned"
    );
    ensure!(
        std::fs::read_dir(workspace)?.all(|entry| entry
            .is_ok_and(|entry| !entry.file_name().to_string_lossy().starts_with(".spacetime-image-"))),
        "deadline returned before its owned workspace was released"
    );
    Ok(())
}
