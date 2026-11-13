//! `spacetime inc` runs `spacetimedb-inc`, the SpacetimeInc CLI, which is built and versioned
//! separately from this one. Like `spacetime start`, it runs a binary installed next to
//! `spacetimedb-cli`. If that binary is missing, it offers to download it first.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::Context;
use bytes::Bytes;
use clap::{Arg, ArgMatches};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

use crate::subcommands::start::exec_replace;
use crate::util::resolve_sibling_binary;

const BIN_NAME: &str = "spacetimedb-inc";
const DEFAULT_MANIFEST_URL: &str = "https://spacetimeinc.com/cli/manifest.json";
/// Overrides [`DEFAULT_MANIFEST_URL`].
const MANIFEST_URL_VAR: &str = "SPACETIMEDB_INC_MANIFEST_URL";
/// Set to `1` to install `spacetimedb-inc` without asking.
/// A flag can't do this, because every argument goes to `spacetimedb-inc`.
const YES_VAR: &str = "SPACETIMEDB_INC_YES";

pub fn cli() -> clap::Command {
    clap::Command::new("inc")
        .about("Run SpacetimeInc commands (installs `spacetimedb-inc` on first use)")
        .disable_help_flag(true)
        .arg(
            Arg::new("args")
                .help("The args to pass to `spacetimedb-inc`")
                .value_parser(clap::value_parser!(OsString))
                .allow_hyphen_values(true)
                .trailing_var_arg(true)
                .num_args(0..),
        )
}

pub async fn exec(args: &ArgMatches) -> anyhow::Result<ExitCode> {
    let forwarded_args: Vec<OsString> = args.get_many::<OsString>("args").unwrap_or_default().cloned().collect();
    let raw_args: Vec<OsString> = std::env::args_os().collect();
    let forwarded_args = restore_leading_escape(&raw_args, forwarded_args);
    let bin_path = resolve_sibling_binary(BIN_NAME)?;
    if !bin_path.exists() && !install(&bin_path).await? {
        return Ok(ExitCode::FAILURE);
    }
    let mut cmd = Command::new(&bin_path);
    cmd.args(&forwarded_args);
    exec_replace(&mut cmd).with_context(|| format!("exec failed for {}", bin_path.display()))
}

/// clap swallows a `--` that comes right after `inc`, so that `spacetime inc -- x` would run
/// `spacetimedb-inc x`. This puts it back. The forwarded args are always a suffix of the
/// process's args, and the only thing clap can consume between `inc` and them is that `--`.
fn restore_leading_escape(raw_args: &[OsString], mut forwarded: Vec<OsString>) -> Vec<OsString> {
    let before = raw_args.len().checked_sub(forwarded.len() + 1).map(|i| &raw_args[i]);
    if raw_args.ends_with(&forwarded) && before.is_some_and(|arg| arg == "--") {
        forwarded.insert(0, "--".into());
    }
    forwarded
}

/// Asks to install `spacetimedb-inc` at `dest`, and installs it if allowed.
/// Returns whether it was installed.
async fn install(dest: &Path) -> anyhow::Result<bool> {
    let manifest_url = std::env::var(MANIFEST_URL_VAR).unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_owned());
    let manifest_url =
        Url::parse(&manifest_url).with_context(|| format!("invalid {MANIFEST_URL_VAR}: {manifest_url}"))?;
    let platform = current_platform()?;
    let host = manifest_url.host_str().unwrap_or(manifest_url.as_str());
    let dir = dest.parent().context("the CLI binary has no parent directory")?;

    if !confirm_install(host, dir)? {
        return Ok(false);
    }

    eprintln!("Downloading {BIN_NAME} for {platform} from {host}...");
    let client = reqwest::Client::builder()
        .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let release = fetch_release(&client, &manifest_url, &platform).await?;
    write_executable(dest, &release.bytes)?;
    eprintln!("Installed {BIN_NAME} {} to {}", release.version, dest.display());
    Ok(true)
}

fn confirm_install(host: &str, dir: &Path) -> anyhow::Result<bool> {
    if is_yes(std::env::var_os(YES_VAR)) {
        return Ok(true);
    }
    if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
        eprintln!(
            "{BIN_NAME} is not installed. Run `spacetime inc` in a terminal to install it, \
             or set {YES_VAR}=1 to install it without asking."
        );
        return Ok(false);
    }
    let yes = dialoguer::Confirm::new()
        .with_prompt(format!(
            "{BIN_NAME} is not installed. Download it from {host} into {}?",
            dir.display()
        ))
        .default(false)
        .interact()?;
    if !yes {
        eprintln!("Not installing {BIN_NAME}.");
    }
    Ok(yes)
}

fn is_yes(value: Option<OsString>) -> bool {
    value.is_some_and(|v| ["1", "true", "yes"].iter().any(|yes| v.eq_ignore_ascii_case(yes)))
}

/// The release manifest at [`DEFAULT_MANIFEST_URL`].
/// Unknown fields are ignored, so the format can grow without breaking older CLIs.
#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    files: Vec<ManifestFile>,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    /// `{os}-{arch}`, as returned by [`platform_name`], e.g. `darwin-arm64`.
    platform: String,
    /// The binary's URL, either absolute or relative to the manifest's.
    url: String,
    /// The binary's SHA-256, hex-encoded.
    sha256: String,
}

impl Manifest {
    fn parse(json: &[u8]) -> anyhow::Result<Self> {
        serde_json::from_slice(json).context("invalid spacetimedb-inc manifest")
    }

    fn file_for(&self, platform: &str) -> anyhow::Result<&ManifestFile> {
        self.files.iter().find(|f| f.platform == platform).with_context(|| {
            let available: Vec<_> = self.files.iter().map(|f| &*f.platform).collect();
            format!(
                "{BIN_NAME} {} has no build for {platform} (available: {})",
                self.version,
                available.join(", ")
            )
        })
    }
}

/// Names a platform the way the manifest does: `{darwin,linux,windows}-{arm64,x64}`.
/// Takes the values of [`std::env::consts::OS`] and [`std::env::consts::ARCH`].
fn platform_name(os: &str, arch: &str) -> Option<String> {
    let os = match os {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => return None,
    };
    let arch = match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return None,
    };
    Some(format!("{os}-{arch}"))
}

fn current_platform() -> anyhow::Result<String> {
    use std::env::consts::{ARCH, OS};
    platform_name(OS, ARCH).with_context(|| format!("{BIN_NAME} is not available for {OS} on {ARCH}"))
}

struct Release {
    version: String,
    bytes: Bytes,
}

/// Downloads the build of `spacetimedb-inc` for `platform` and checks its SHA-256.
async fn fetch_release(client: &reqwest::Client, manifest_url: &Url, platform: &str) -> anyhow::Result<Release> {
    let manifest = get(client, manifest_url)
        .await
        .with_context(|| format!("could not fetch the {BIN_NAME} manifest"))?;
    let manifest = Manifest::parse(&manifest)?;
    let file = manifest.file_for(platform)?;
    let url = manifest_url
        .join(&file.url)
        .with_context(|| format!("invalid URL in the {BIN_NAME} manifest: {}", file.url))?;
    let bytes = get(client, &url)
        .await
        .with_context(|| format!("could not download {BIN_NAME}"))?;
    verify_sha256(&bytes, &file.sha256)?;
    Ok(Release {
        version: manifest.version,
        bytes,
    })
}

async fn get(client: &reqwest::Client, url: &Url) -> anyhow::Result<Bytes> {
    Ok(client
        .get(url.clone())
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?)
}

fn verify_sha256(bytes: &[u8], expected: &str) -> anyhow::Result<()> {
    let actual: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    anyhow::ensure!(
        actual.eq_ignore_ascii_case(expected.trim()),
        "the downloaded {BIN_NAME} does not match the manifest's checksum (expected SHA-256 {expected}, got {actual})"
    );
    Ok(())
}

/// Writes an executable to `dest`, through a temporary file in the same directory,
/// so that `dest` never holds a partial binary.
fn write_executable(dest: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let dir = dest.parent().context("the install path has no parent directory")?;
    let mut file =
        tempfile::NamedTempFile::new_in(dir).with_context(|| format!("could not write to {}", dir.display()))?;
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file().set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    file.persist(dest)
        .with_context(|| format!("could not install {BIN_NAME} to {}", dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn manifest_json(sha256: &str) -> String {
        format!(
            r#"{{
                "version": "0.3.0",
                "released": "2026-11-12",
                "files": [
                    {{ "platform": "linux-x64", "url": "https://example.com/linux-x64", "sha256": "00", "size": 5 }},
                    {{ "platform": "darwin-arm64", "url": "bin/darwin-arm64", "sha256": "{sha256}", "size": 5 }}
                ]
            }}"#
        )
    }

    #[test]
    fn parses_manifest_and_ignores_unknown_fields() {
        let manifest = Manifest::parse(manifest_json(HELLO_SHA256).as_bytes()).unwrap();
        assert_eq!(manifest.version, "0.3.0");
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.files[1].url, "bin/darwin-arm64");
        assert_eq!(manifest.files[1].sha256, HELLO_SHA256);
    }

    #[test]
    fn rejects_manifest_missing_fields() {
        assert!(Manifest::parse(br#"{ "version": "0.3.0" }"#).is_err());
        assert!(Manifest::parse(br#"{ "version": "0.3.0", "files": [{ "platform": "linux-x64" }] }"#).is_err());
        assert!(Manifest::parse(b"not json").is_err());
    }

    #[test]
    fn names_platforms() {
        assert_eq!(platform_name("macos", "aarch64").as_deref(), Some("darwin-arm64"));
        assert_eq!(platform_name("macos", "x86_64").as_deref(), Some("darwin-x64"));
        assert_eq!(platform_name("linux", "x86_64").as_deref(), Some("linux-x64"));
        assert_eq!(platform_name("linux", "aarch64").as_deref(), Some("linux-arm64"));
        assert_eq!(platform_name("windows", "x86_64").as_deref(), Some("windows-x64"));
        assert_eq!(platform_name("freebsd", "x86_64"), None);
        assert_eq!(platform_name("linux", "riscv64"), None);
    }

    #[test]
    fn selects_file_for_platform() {
        let manifest = Manifest::parse(manifest_json(HELLO_SHA256).as_bytes()).unwrap();
        assert_eq!(
            manifest.file_for("linux-x64").unwrap().url,
            "https://example.com/linux-x64"
        );
        assert_eq!(manifest.file_for("darwin-arm64").unwrap().url, "bin/darwin-arm64");
        let err = format!("{:#}", manifest.file_for("windows-x64").unwrap_err());
        assert!(err.contains("no build for windows-x64"), "{err}");
        assert!(err.contains("linux-x64, darwin-arm64"), "{err}");
    }

    #[test]
    fn verifies_sha256() {
        verify_sha256(b"hello", HELLO_SHA256).unwrap();
        verify_sha256(b"hello", &HELLO_SHA256.to_uppercase()).unwrap();
        let err = format!("{:#}", verify_sha256(b"hellO", HELLO_SHA256).unwrap_err());
        assert!(err.contains("does not match"), "{err}");
        assert!(verify_sha256(b"hello", "").is_err());
    }

    /// Parses `argv` with the real `spacetime` subcommand, as `main` does, and returns what `exec` forwards.
    fn forwarded(argv: &[&str]) -> Vec<String> {
        let matches = clap::Command::new("spacetime")
            .subcommand(cli())
            .try_get_matches_from(argv)
            .unwrap();
        let (_, args) = matches.subcommand().unwrap();
        let forwarded = args.get_many::<OsString>("args").unwrap_or_default().cloned().collect();
        let raw: Vec<OsString> = argv.iter().map(Into::into).collect();
        restore_leading_escape(&raw, forwarded)
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect()
    }

    #[test]
    fn forwards_args_verbatim() {
        for args in [
            &[][..],
            &["--help"],
            &["-h"],
            &["--version"],
            &["tasks", "list", "--json"],
            &["--yes", "run", "--", "two words"],
            &["--"],
            &["--", "--help"],
            &["--", "--", "x"],
        ] {
            let argv: Vec<&str> = ["spacetime", "inc"].iter().chain(args).copied().collect();
            assert_eq!(forwarded(&argv), args, "spacetime inc {args:?}");
        }
    }

    #[test]
    fn reads_yes_var() {
        assert!(is_yes(Some("1".into())));
        assert!(is_yes(Some("TRUE".into())));
        assert!(is_yes(Some("yes".into())));
        assert!(!is_yes(None));
        assert!(!is_yes(Some("".into())));
        assert!(!is_yes(Some("0".into())));
        assert!(!is_yes(Some("no".into())));
    }

    /// Serves `routes` over HTTP on loopback, and returns the URL of `/manifest.json`.
    async fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> Url {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let mut chunk = [0; 1024];
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split_whitespace().nth(1).unwrap();
                let (status, body) = match routes.iter().find(|(route, _)| *route == path) {
                    Some((_, body)) => ("200 OK", body.clone()),
                    None => ("404 Not Found", Vec::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
                stream.shutdown().await.unwrap();
            }
        });
        Url::parse(&format!("http://{address}/manifest.json")).unwrap()
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[tokio::test]
    async fn downloads_and_installs_from_relative_url() {
        let manifest_url = serve(vec![
            ("/manifest.json", manifest_json(HELLO_SHA256).into_bytes()),
            ("/bin/darwin-arm64", b"hello".to_vec()),
        ])
        .await;
        let release = fetch_release(&test_client(), &manifest_url, "darwin-arm64")
            .await
            .unwrap();
        assert_eq!(release.version, "0.3.0");
        assert_eq!(&*release.bytes, b"hello");

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(BIN_NAME);
        write_executable(&dest, &release.bytes).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o755);
        }
    }

    #[tokio::test]
    async fn rejects_download_with_wrong_checksum() {
        let manifest_url = serve(vec![
            ("/manifest.json", manifest_json(HELLO_SHA256).into_bytes()),
            ("/bin/darwin-arm64", b"tampered".to_vec()),
        ])
        .await;
        let err = fetch_release(&test_client(), &manifest_url, "darwin-arm64")
            .await
            .err()
            .unwrap();
        let err = format!("{err:#}");
        assert!(err.contains("does not match the manifest's checksum"), "{err}");
    }
}
