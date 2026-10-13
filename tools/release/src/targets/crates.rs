use crate::targets::{util, ReleaseTarget};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const CRATE_OWNERS: &[&str] = &["cloutiertyler", "jdetter", "bfops", "rekhoff", "spacetimedb-devops"];
const CRATES_IO_API: &str = "https://crates.io/api/v1";

pub struct CratesRelease {
    pub version: String,
    pub dry_run: bool,
}

impl CratesRelease {
    pub fn new(version: String, dry_run: bool) -> Self {
        Self { version, dry_run }
    }

    fn publish_crates(&self, workspace_root: &Path, crates: &[&String]) -> Result<(), String> {
        if crates.is_empty() {
            println!("All selected crates are already published; nothing to publish.");
            return Ok(());
        }
        // Workaround for some weird bug where dry runs fail if you don't run them in a sterile home dir
        let cargo_home = if self.dry_run {
            Some(tempfile::tempdir().map_err(|err| format!("Failed to create temporary Cargo home: {err}"))?)
        } else {
            None
        };
        // NOTE: We must keep all crates in one publish invocation.
        // Without this, dry-run doesn't work properly: every crate will look for its deps on crates.io
        // and fail, but if they're on one invocation then they're able to resolve properly.
        let mut cmd = Command::new("cargo");
        cmd.current_dir(workspace_root).args(["publish", "--allow-dirty"]);
        for crate_name in crates {
            cmd.args(["-p", crate_name]);
        }
        if self.dry_run {
            cmd.arg("--dry-run");
        }
        if let Some(home) = &cargo_home {
            cmd.env("CARGO_HOME", home.path());
        }
        util::print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|err| format!("Failed to execute cargo publish: {err}"))?;
        if !status.success() {
            return Err(format!("cargo publish failed: {status}"));
        }
        Ok(())
    }

    fn add_crate_owners(&self, crate_name: &str) -> Result<(), String> {
        println!("Adding owners for crate: {}", crate_name);

        for owner in CRATE_OWNERS {
            let mut cmd = Command::new("cargo");
            cmd.args(["owner", "--add", owner, crate_name]);
            util::print_command(&cmd);

            let output = cmd
                .output()
                .map_err(|e| format!("Failed to execute cargo owner --add {} {}: {}", owner, crate_name, e))?;

            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            if output.status.success() {
                println!("Added {} as an owner of {}.", owner, crate_name);
                continue;
            }

            if stderr.contains("already") || stdout.contains("already") {
                println!("{} is already an owner of {}.", owner, crate_name);
                continue;
            }

            return Err(format!(
                "Failed to add owner {} to crate {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                owner, crate_name, stdout, stderr
            ));
        }

        Ok(())
    }
}

fn is_published(client: &Client, api: &str, crate_name: &str, version: &str) -> Result<bool, String> {
    let url = format!("{}/crates/{crate_name}/{version}", api.trim_end_matches('/'));
    let response = client
        .get(&url)
        .send()
        .map_err(|err| format!("Failed to query {url}: {err}"))?;
    match response.status() {
        StatusCode::OK => Ok(true),
        StatusCode::NOT_FOUND => Ok(false),
        status => Err(format!("Failed to query {url}: {status}")),
    }
}

impl ReleaseTarget for CratesRelease {
    fn release(&self) -> Result<(), String> {
        println!("Finding crates to publish to crates.io...");

        let workspace_root = crate::crates_resolver::find_workspace_root()
            .map_err(|e| format!("Failed to find workspace root: {}", e))?;
        let crates = crate::crates_resolver::get_crates_to_publish()
            .map_err(|e| format!("Failed to find crates to publish: {}", e))?;
        println!("\nCrates to publish in order:");
        for crate_name in &crates {
            println!("  - {}", crate_name);
        }
        if self.dry_run {
            println!("\nDRY RUN: No crates will be published");
        }

        println!("\nStarting publish process...");
        let client = Client::builder()
            .user_agent("spacetimedb-release (https://github.com/clockworklabs/SpacetimeDB)")
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| format!("Failed to create crates.io API client: {err}"))?;
        let crates_io_version = self.version.strip_prefix('v').unwrap_or(&self.version);
        let mut pending = Vec::new();
        for crate_name in &crates {
            // Pace requests to respect crates.io's API guidance.
            std::thread::sleep(Duration::from_secs(1));
            if is_published(&client, CRATES_IO_API, crate_name, crates_io_version)? {
                println!(
                    "Crate {} already published on crates.io; skipping (treating as success).",
                    crate_name
                );
            } else {
                pending.push(crate_name);
            }
        }
        self.publish_crates(&workspace_root, &pending)?;
        if !self.dry_run {
            for crate_name in &crates {
                self.add_crate_owners(crate_name)?;
            }
        }
        if self.dry_run {
            println!("\nDry run completed successfully!");
        } else {
            println!("\nAll crates published successfully!");
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "crates"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn serve_once(status: &str, body: &str) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = [0; 4096];
            let len = socket.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..len]).starts_with("GET /crates/example/1.2.3 "));
            socket.write_all(response.as_bytes()).unwrap();
        });
        (url, thread)
    }

    #[test]
    fn version_endpoint_distinguishes_missing_versions_from_failures() {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        for (status, expected) in [
            ("200 OK", Some(true)),
            ("404 Not Found", Some(false)),
            ("403 Forbidden", None),
            ("429 Too Many Requests", None),
            ("503 Service Unavailable", None),
            ("202 Accepted", None),
        ] {
            let (url, thread) = serve_once(status, "");
            assert_eq!(is_published(&client, &url, "example", "1.2.3").ok(), expected);
            thread.join().unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        assert!(is_published(&client, &url, "example", "1.2.3").is_err());
    }

    #[test]
    fn empty_selection_skips_cargo_in_both_modes() {
        for dry_run in [false, true] {
            // A nonexistent workspace proves we return before invoking Cargo.
            CratesRelease::new("1.2.3".into(), dry_run)
                .publish_crates(Path::new("/nonexistent-release-workspace"), &[])
                .unwrap();
        }
    }

    #[test]
    #[ignore = "downloads crates.io index; run explicitly for release validation"]
    fn dry_run_verifies_unpublished_dependencies_and_propagates_build_failure() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"base\", \"dependent\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        let version = "0.0.0-release-dry-run-test";
        let base = String::from("spacetimedb-release-fixture-base");
        let dependent = String::from("spacetimedb-release-fixture-dependent");
        for (dir, package, source) in [
            ("base", &base, "pub fn value() -> u8 { 42 }"),
            (
                "dependent",
                &dependent,
                "pub fn value() -> u8 { spacetimedb_release_fixture_base::value() }",
            ),
        ] {
            let path = workspace.path().join(dir);
            std::fs::create_dir_all(path.join("src")).unwrap();
            let mut manifest = format!("[package]\nname = {:?}\nversion = {:?}\nedition = \"2021\"\nlicense = \"MIT\"\ndescription = \"Release validation fixture\"\n", package, version);
            if dir == "dependent" {
                manifest.push_str(&format!(
                    "[dependencies]\n{} = {{ path = \"../base\", version = \"={}\" }}\n",
                    base, version
                ));
            }
            std::fs::write(path.join("Cargo.toml"), manifest).unwrap();
            std::fs::write(path.join("src/lib.rs"), source).unwrap();
        }
        let release = CratesRelease::new(version.into(), true);
        release.publish_crates(workspace.path(), &[&base, &dependent]).unwrap();
        std::fs::write(
            workspace.path().join("dependent/src/lib.rs"),
            "compile_error!(\"intentional release validation failure\");",
        )
        .unwrap();
        assert!(release.publish_crates(workspace.path(), &[&base, &dependent]).is_err());
    }
}
