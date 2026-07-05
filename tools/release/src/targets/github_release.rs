use crate::targets::ReleaseTarget;
use duct::cmd;
use serde::Deserialize;

const REPO: &str = "clockworklabs/SpacetimeDB";

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

    fn fetch_release(&self) -> Result<Release, String> {
        let cmd = cmd!(
            "gh",
            "release",
            "view",
            &self.version,
            "--repo",
            REPO,
            "--json",
            "isDraft,url"
        );
        println!("$> {:?}", cmd);
        let output = cmd
            .read()
            .map(|s| s.trim().to_owned())
            .map_err(|e| format!("Failed to execute view GitHub release: {e}"))?;
        serde_json::from_str(&output).map_err(|err| format!("Failed to parse GitHub release JSON: {}", err))
    }

    fn dispatch_attach_artifacts(&self) -> Result<String, String> {
        let release_tag = format!("release_tag={}", self.version);
        let cmd = cmd!(
            "gh",
            "workflow",
            "run",
            "attach-artifacts.yml",
            "--repo",
            REPO,
            "--ref",
            "master",
            "-f",
            &release_tag,
        );

        println!("$> {:?}", cmd);
        cmd.read()
            .map(|s| s.trim().to_owned())
            .map_err(|e| format!("Failed to execute dispatch attach-artifacts.yml: {e}"))
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
        let cmd = cmd!("gh", "run", "watch", run_id, "--repo", REPO, "--exit-status");
        println!("$> {:?}", cmd);
        cmd.run()
            .map(|_| ())
            .map_err(|e| format!("Failed to execute watch attach-artifacts.yml: {e}"))
    }

    fn publish_release(&self) -> Result<(), String> {
        let release_endpoint = format!("repos/{}/releases/tags/{}", REPO, self.version);
        let id_cmd = cmd!("gh", "api", &release_endpoint, "--jq", ".id");
        println!("$> {:?}", id_cmd);
        let release_id = id_cmd
            .read()
            .map(|s| s.trim().to_owned())
            .map_err(|e| format!("Failed to execute get GitHub release id: {e}"))?;

        let publish_endpoint = format!("repos/{}/releases/{}", REPO, release_id);
        let publish_cmd = cmd!("gh", "api", "--method", "PATCH", &publish_endpoint, "-F", "draft=false");
        println!("$> {:?}", publish_cmd);
        publish_cmd
            .run()
            .map(|_| ())
            .map_err(|e| format!("Failed to execute publish GitHub release: {e}"))
    }
}

impl ReleaseTarget for GithubRelease {
    fn release(&self) -> Result<(), String> {
        let release = self.fetch_release()?;
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
