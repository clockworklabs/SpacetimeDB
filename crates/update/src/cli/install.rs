use std::io;

use anyhow::Context;
use bytes::{Buf, Bytes};
use http_body_util::BodyExt;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use spacetimedb_paths::SpacetimePaths;

use super::ForceYes;

/// Install a specific SpacetimeDB version.
#[derive(clap::Args)]
pub(super) struct Install {
    /// The SpacetimeDB version to install.
    version: semver::Version,

    /// The SpacetimeDB edition(s) to install, separated by commas.
    #[arg(long, value_delimiter = ',', action = clap::ArgAction::Set, default_value = "standalone")]
    edition: Vec<String>,

    /// Switch to this version after it is installed.
    #[arg(long)]
    r#use: bool,

    /// The name of the release artifact to download from github.
    #[arg(long, hide = true)]
    artifact_name: Option<String>,

    #[command(flatten)]
    yes: ForceYes,
}

impl Install {
    pub(super) fn exec(self, paths: &SpacetimePaths) -> anyhow::Result<()> {
        super::tokio_block_on(async {
            anyhow::ensure!(
                self.edition == ["standalone"],
                "can only install spacetimedb-standalone at the moment"
            );
            let client = super::reqwest_client()?;
            let version = download_and_install(&client, Some(self.version), self.artifact_name, paths).await?;
            if self.r#use {
                paths.cli_bin_dir.set_current_version(&version)?;
            }
            Ok(())
        })?
    }
}

pub(super) async fn download_and_install(
    client: &reqwest::Client,
    version: Option<semver::Version>,
    artifact_name: Option<String>,
    paths: &SpacetimePaths,
) -> anyhow::Result<semver::Version> {
    let custom_artifact = artifact_name.is_some();
    let download_name = artifact_name.unwrap_or_else(get_download_name);
    let artifact_type = ArtifactType::deduce(&download_name).context("Unknown archive type")?;

    let pb_style = ProgressStyle::with_template("{spinner} {prefix}{msg}").unwrap();
    let pb = ProgressBar::new(0).with_style(pb_style.clone());
    pb.enable_steady_tick(std::time::Duration::from_millis(60));

    pb.set_message("Resolving version...");
    let releases_url = "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases";
    let url = match &version {
        Some(version) => format!("{releases_url}/tags/v{version}"),
        None => [releases_url, "/latest"].concat(),
    };
    let release: Release = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| {
            if e.status() == Some(reqwest::StatusCode::NOT_FOUND) {
                if let Some(version) = &version {
                    return anyhow::anyhow!(e).context(format!("No release found for version {version}"));
                }
            }
            anyhow::anyhow!(e).context("Could not fetch release info")
        })?
        .json()
        .await?;
    let release_version = match version {
        Some(version) => version,
        None => release.version().context("Could not parse version number")?,
    };

    let asset = release
        .assets
        .iter()
        .find(|&asset| asset.name == download_name)
        .ok_or_else(|| {
            let err = anyhow::anyhow!("artifact named {download_name} not found in version {release_version}");
            if custom_artifact {
                err
            } else {
                err.context("no prebuilt binaries available for the detected OS and architecture")
            }
        })?;

    pb.set_style(ProgressStyle::with_template("{spinner} {prefix}{msg} {bytes}/{total_bytes} ({eta})").unwrap());
    pb.set_prefix(format!("Installing v{release_version}: "));
    pb.set_message("downloading...");
    let archive = download_with_progress(&pb, client, &asset.browser_download_url).await?;

    pb.set_style(pb_style);
    pb.set_message("unpacking...");

    let version_dir = paths.cli_bin_dir.version_dir(&release_version);
    match artifact_type {
        ArtifactType::TarGz => {
            let tgz = archive.aggregate().reader();
            tar::Archive::new(flate2::bufread::GzDecoder::new(tgz)).unpack(&version_dir)?;
        }
        ArtifactType::Zip => {
            let zip = archive.to_bytes();
            let zip = io::Cursor::new(&*zip);
            zip::ZipArchive::new(zip)?.extract(&version_dir)?;
        }
    }

    pb.finish_with_message("done!");

    Ok(release_version)
}

enum ArtifactType {
    TarGz,
    Zip,
}

impl ArtifactType {
    fn deduce(filename: &str) -> Option<Self> {
        if filename.ends_with(".tar.gz") {
            Some(Self::TarGz)
        } else if filename.ends_with(".zip") {
            Some(Self::Zip)
        } else {
            None
        }
    }
}

pub(super) async fn available_releases(client: &reqwest::Client) -> anyhow::Result<Vec<semver::Version>> {
    let url = "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases";
    let releases: Vec<Release> = client.get(url).send().await?.json().await?;

    releases.into_iter().map(|release| release.version()).collect()
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

impl Release {
    fn version(&self) -> anyhow::Result<semver::Version> {
        let ver = self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name);
        Ok(semver::Version::parse(ver)?)
    }
}

async fn download_with_progress(
    pb: &ProgressBar,
    client: &reqwest::Client,
    url: &str,
) -> Result<http_body_util::Collected<Bytes>, anyhow::Error> {
    let response = client.get(url).send().await?.error_for_status()?;

    pb.set_length(response.content_length().unwrap_or(0));

    let body = reqwest::Body::from(response)
        .map_frame(|f| {
            if let Some(data) = f.data_ref() {
                pb.inc(data.len() as u64);
            }
            f
        })
        .collect()
        .await?;

    Ok(body)
}

fn get_download_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        os => os,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        arch => arch,
    };
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    format!("spacetime-{os}-{arch}.{ext}")
}