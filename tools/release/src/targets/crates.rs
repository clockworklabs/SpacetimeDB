use crate::crates_resolver::CrateToPublish;
use crate::targets::{util, ReleaseTarget};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const CRATE_OWNERS: &[&str] = &["cloutiertyler", "jdetter", "bfops", "rekhoff", "spacetimedb-devops"];
const CRATES_IO_INDEX: &str = "https://index.crates.io";

pub struct CratesRelease {
    pub dry_run: bool,
}

impl CratesRelease {
    // Publishing uses the manifest versions; the release tag is shared with other targets.
    pub fn new(_version: String, dry_run: bool) -> Self {
        Self { dry_run }
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

#[derive(Deserialize)]
struct IndexEntry {
    name: String,
    vers: String,
}

fn index_path(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

fn is_published(client: &Client, index: &str, krate: &CrateToPublish) -> Result<bool, String> {
    let url = format!("{}/{}", index.trim_end_matches('/'), index_path(&krate.name));
    let response = client
        .get(&url)
        .send()
        .map_err(|err| format!("Failed to query {url}: {err}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(false);
    }
    let body = response
        .error_for_status()
        .and_then(|response| response.text())
        .map_err(|err| format!("Failed to read {url}: {err}"))?;
    index_contains_version(&body, krate).map_err(|err| format!("Invalid index response from {url}: {err}"))
}

fn index_contains_version(body: &str, krate: &CrateToPublish) -> Result<bool, String> {
    if body.is_empty() {
        return Err("empty index entry".into());
    }
    let mut found = false;
    for line in body.lines() {
        let entry: IndexEntry = serde_json::from_str(line).map_err(|err| err.to_string())?;
        if entry.name != krate.name {
            return Err(format!("expected package {}, got {}", krate.name, entry.name));
        }
        found |= entry.vers == krate.version;
    }
    Ok(found)
}

fn unpublished_crates(
    crates: &[CrateToPublish],
    mut published: impl FnMut(&CrateToPublish) -> Result<bool, String>,
) -> Result<Vec<&CrateToPublish>, String> {
    let mut pending = Vec::new();
    for krate in crates {
        if published(krate)? {
            println!("{}@{} is already published; skipping.", krate.name, krate.version);
        } else {
            pending.push(krate);
        }
    }
    Ok(pending)
}

fn publish_command(workspace_root: &Path, crates: &[&CrateToPublish], dry_run: bool) -> Command {
    // Keep all crates in one invocation so --dry-run can verify unpublished dependencies
    // using Cargo's temporary registry; separate invocations would look for them on crates.io.
    let mut cmd = Command::new("cargo");
    cmd.current_dir(workspace_root).args(["publish", "--allow-dirty"]);
    for krate in crates {
        cmd.args(["-p", &krate.name]);
    }
    if dry_run {
        cmd.arg("--dry-run");
    }
    cmd
}

fn publish_crates(workspace_root: &Path, crates: &[&CrateToPublish], dry_run: bool) -> Result<(), String> {
    if crates.is_empty() {
        println!("All selected crate versions are already published; nothing to publish.");
        return Ok(());
    }
    let cargo_home = if dry_run {
        Some(tempfile::tempdir().map_err(|err| format!("Failed to create temporary Cargo home: {err}"))?)
    } else {
        None
    };
    let mut cmd = publish_command(workspace_root, crates, dry_run);
    if let Some(home) = &cargo_home {
        cmd.env("CARGO_HOME", home.path());
        println!(
            "DRY RUN: Verifying packages with temporary CARGO_HOME={}",
            home.path().display()
        );
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

impl ReleaseTarget for CratesRelease {
    fn release(&self) -> Result<(), String> {
        let workspace_root = crate::crates_resolver::find_workspace_root()
            .map_err(|err| format!("Failed to find workspace root: {err}"))?;
        let crates = crate::crates_resolver::get_crates_to_publish(&workspace_root)
            .map_err(|err| format!("Failed to find crates to publish: {err}"))?;
        println!("Crates selected for release:");
        for krate in &crates {
            println!("  - {}@{}", krate.name, krate.version);
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|err| format!("Failed to create crates.io index client: {err}"))?;
        let pending = unpublished_crates(&crates, |krate| is_published(&client, CRATES_IO_INDEX, krate))?;
        publish_crates(&workspace_root, &pending, self.dry_run)?;
        if !self.dry_run {
            for krate in &crates {
                self.add_crate_owners(&krate.name)?;
            }
        }
        println!(
            "Crate release{} completed successfully!",
            if self.dry_run { " dry-run" } else { "" }
        );
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

    fn krate(name: &str, version: &str) -> CrateToPublish {
        CrateToPublish {
            name: name.into(),
            version: version.into(),
        }
    }

    #[test]
    fn sparse_index_paths() {
        for (name, expected) in [
            ("A", "1/a"),
            ("ab", "2/ab"),
            ("abc", "3/a/abc"),
            ("serde", "se/rd/serde"),
        ] {
            assert_eq!(index_path(name), expected);
        }
    }

    #[test]
    fn exact_versions_and_malformed_entries() {
        let package = krate("example", "1.2.3");
        let body = "{\"name\":\"example\",\"vers\":\"1.2.2\"}\n{\"name\":\"example\",\"vers\":\"1.2.3\"}\n";
        assert!(index_contains_version(body, &package).unwrap());
        assert!(!index_contains_version(body, &krate("example", "1.2.3-rc.1")).unwrap());
        for invalid in ["", "not JSON", "{}", "{\"name\":\"other\",\"vers\":\"1.2.3\"}"] {
            assert!(index_contains_version(invalid, &package).is_err());
        }
        // Do not silently accept a corrupt response after a matching record.
        assert!(index_contains_version(&format!("{body}not JSON"), &package).is_err());
    }

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
            assert!(String::from_utf8_lossy(&request[..len]).starts_with("GET /ex/am/example "));
            socket.write_all(response.as_bytes()).unwrap();
        });
        (url, thread)
    }

    #[test]
    fn index_http_failures_are_not_unpublished_versions() {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        for (status, body, expected) in [
            ("200 OK", "{\"name\":\"example\",\"vers\":\"1.2.3\"}\n", Some(true)),
            ("200 OK", "{\"name\":\"example\",\"vers\":\"1.2.2\"}\n", Some(false)),
            ("404 Not Found", "missing", Some(false)),
            ("500 Internal Server Error", "unavailable", None),
            ("403 Forbidden", "denied", None),
            ("200 OK", "broken", None),
        ] {
            let (url, thread) = serve_once(status, body);
            let result = is_published(&client, &url, &krate("example", "1.2.3"));
            assert_eq!(result.ok(), expected);
            thread.join().unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        assert!(is_published(&client, &url, &krate("example", "1.2.3")).is_err());
    }

    #[test]
    fn both_modes_filter_the_same_versions() {
        let crates = [krate("first", "1.2.3"), krate("second", "2.0.0")];
        for dry_run in [false, true] {
            for published in [[false, false], [true, false], [true, true]] {
                let mut checked = 0;
                let pending = unpublished_crates(&crates, |_| {
                    let result = published[checked];
                    checked += 1;
                    Ok(result)
                })
                .unwrap();
                assert_eq!(checked, 2);
                assert_eq!(
                    pending,
                    crates
                        .iter()
                        .zip(published)
                        .filter_map(|(krate, published)| (!published).then_some(krate))
                        .collect::<Vec<_>>()
                );
                let args: Vec<_> = publish_command(Path::new("."), &pending, dry_run)
                    .get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect();
                assert_eq!(args.contains(&"--dry-run".into()), dry_run);
                assert!(!args.contains(&"--no-verify".into()));
                assert!(!args.contains(&"--registry".into()));
                if pending.is_empty() {
                    // A nonexistent workspace proves we return before invoking Cargo.
                    publish_crates(Path::new("/nonexistent-release-workspace"), &pending, dry_run).unwrap();
                }
            }
            assert!(unpublished_crates(&crates, |_| Err("index unavailable".into())).is_err());
        }
    }

    #[test]
    fn publish_commands_differ_only_by_dry_run_flag() {
        let crates = [krate("first", "1.2.3"), krate("second", "2.0.0")];
        let pending: Vec<_> = crates.iter().collect();
        let live = publish_command(Path::new("/workspace"), &pending, false);
        let dry = publish_command(Path::new("/workspace"), &pending, true);
        let mut expected: Vec<_> = live.get_args().collect();
        expected.push(std::ffi::OsStr::new("--dry-run"));
        assert_eq!(dry.get_args().collect::<Vec<_>>(), expected);
        assert_eq!(live.get_current_dir(), dry.get_current_dir());
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
        let base = krate("spacetimedb-release-fixture-base", version);
        let dependent = krate("spacetimedb-release-fixture-dependent", version);
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
            let mut manifest = format!("[package]\nname = {:?}\nversion = {:?}\nedition = \"2021\"\nlicense = \"MIT\"\ndescription = \"Release validation fixture\"\n", package.name, package.version);
            if dir == "dependent" {
                manifest.push_str(&format!(
                    "[dependencies]\n{} = {{ path = \"../base\", version = \"={}\" }}\n",
                    base.name, version
                ));
            }
            std::fs::write(path.join("Cargo.toml"), manifest).unwrap();
            std::fs::write(path.join("src/lib.rs"), source).unwrap();
        }
        publish_crates(workspace.path(), &[&base, &dependent], true).unwrap();
        std::fs::write(
            workspace.path().join("dependent/src/lib.rs"),
            "compile_error!(\"intentional release validation failure\");",
        )
        .unwrap();
        assert!(publish_crates(workspace.path(), &[&base, &dependent], true).is_err());
    }
}
