use crate::targets::{util, ReleaseTarget};
use serde::Deserialize;
use std::process::Command;

const REPO: &str = "clockworklabs/SpacetimeDB";

fn gh() -> Command {
    Command::new("gh")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    is_draft: bool,
    url: String,
}

pub struct GithubRelease {
    version: String,
}

impl GithubRelease {
    pub fn new(version: String) -> Self {
        Self { version }
    }

    fn run_output(&self, mut cmd: Command, label: &str) -> Result<String, String> {
        util::print_command(&cmd);

        let output = cmd
            .output()
            .map_err(|err| format!("Failed to execute {}: {}", label, err))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                return Ok(stdout);
            }
            return Ok(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }

        Err(format!(
            "{} failed\n--- stdout ---\n{}\n--- stderr ---\n{}",
            label,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }

    fn run_status(&self, mut cmd: Command, label: &str) -> Result<(), String> {
        util::print_command(&cmd);

        let status = cmd
            .status()
            .map_err(|err| format!("Failed to execute {}: {}", label, err))?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("{} failed with status {}", label, status))
        }
    }

    fn release(&self) -> Result<Release, String> {
        let mut cmd = gh();
        cmd.args([
            "release",
            "view",
            &self.version,
            "--repo",
            REPO,
            "--json",
            "isDraft,url",
        ]);

        let output = self.run_output(cmd, "view GitHub release")?;
        serde_json::from_str(&output).map_err(|err| format!("Failed to parse GitHub release JSON: {}", err))
    }

    fn dispatch_attach_artifacts(&self) -> Result<String, String> {
        let mut cmd = gh();
        cmd.args([
            "workflow",
            "run",
            "attach-artifacts.yml",
            "--repo",
            REPO,
            "--ref",
            "master",
            "-f",
            &format!("release_tag={}", self.version),
        ]);

        self.run_output(cmd, "dispatch attach-artifacts.yml")
    }

    fn run_id_from_output<'a>(&self, output: &'a str) -> Result<&'a str, String> {
        let url = output
            .split_whitespace()
            .find(|word| word.starts_with("https://") && word.contains("/actions/runs/"))
            .unwrap_or(output);

        url.trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty() && segment.chars().all(|ch| ch.is_ascii_digit()))
            .ok_or_else(|| {
                format!(
                    "Could not parse workflow run id from gh workflow run output: {}",
                    output
                )
            })
    }

    fn wait_for_artifacts(&self, workflow_output: &str) -> Result<(), String> {
        let run_id = self.run_id_from_output(workflow_output)?;
        let mut cmd = gh();
        cmd.args(["run", "watch", run_id, "--repo", REPO, "--exit-status"]);
        self.run_status(cmd, "watch attach-artifacts.yml")
    }

    fn publish_release(&self) -> Result<(), String> {
        let mut id_cmd = gh();
        id_cmd.args([
            "api",
            &format!("repos/{}/releases/tags/{}", REPO, self.version),
            "--jq",
            ".id",
        ]);
        let release_id = self.run_output(id_cmd, "get GitHub release id")?;

        let mut publish_cmd = gh();
        publish_cmd.args([
            "api",
            "--method",
            "PATCH",
            &format!("repos/{}/releases/{}", REPO, release_id),
            "-F",
            "draft=false",
        ]);
        self.run_status(publish_cmd, "publish GitHub release")
    }
}

impl ReleaseTarget for GithubRelease {
    fn release(&self) -> Result<(), String> {
        let release = self.release()?;
        if !release.is_draft {
            println!("GitHub release {} is already published: {}", self.version, release.url);
            return Ok(());
        }

        println!("Found draft GitHub release {}: {}", self.version, release.url);
        let run_url = self.dispatch_attach_artifacts()?;
        self.wait_for_artifacts(&run_url)?;
        self.publish_release()?;
        println!("Published GitHub release {}.", self.version);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "github-release"
    }
}
