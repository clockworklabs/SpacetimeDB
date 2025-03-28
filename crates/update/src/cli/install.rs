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
            let (version, _) = download_and_install(&client, Some(self.version), self.artifact_name, paths).await?;
            if self.r#use {
                paths.cli_bin_dir.set_current_version(&version.to_string())?;
            }
            Ok(())
        })?
    }
}

pub(super) fn make_progress_bar() -> ProgressBar {
    let pb = ProgressBar::new(0).with_style(ProgressStyle::with_template("{spinner} {prefix}{msg}").unwrap());
    pb.enable_steady_tick(std::time::Duration::from_millis(60));
    pb
}

fn releases_url() -> String {
    std::env::var("SPACETIME_UPDATE_RELEASES_URL")
        .unwrap_or_else(|_| "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases".to_owned())
}

pub(super) async fn download_and_install(
    client: &reqwest::Client,
    version: Option<semver::Version>,
    artifact_name: Option<String>,
    paths: &SpacetimePaths,
) -> anyhow::Result<(semver::Version, Release)> {
    let custom_artifact = artifact_name.is_some();
    let download_name = artifact_name.as_deref().unwrap_or(DOWNLOAD_NAME);
    let artifact_type = ArtifactType::deduce(download_name).context("Unknown archive type")?;

    let pb = make_progress_bar();

    pb.set_message("Resolving version...");
    let releases_url = releases_url();
    let url = match &version {
        Some(version) => format!("{releases_url}/tags/v{version}"),
        None => [&*releases_url, "/latest"].concat(),
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

    pb.set_prefix(format!("Installing v{release_version}: "));
    pb.set_message("downloading...");
    let archive = download_with_progress(&pb, client, &asset.browser_download_url).await?;

    pb.set_message("unpacking...");

    let version_dir = paths.cli_bin_dir.version_dir(&release_version.to_string());
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

    Ok((release_version, release))
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

pub(super) async fn available_releases(client: &reqwest::Client) -> anyhow::Result<Vec<String>> {
    let url = releases_url();
    let releases: Vec<Release> = client.get(url).send().await?.json().await?;

    releases
        .into_iter()
        .map(|release| Ok(release.version()?.to_string()))
        .collect()
}

#[derive(Deserialize)]
pub(super) struct ReleaseAsset {
    pub(super) name: String,
    pub(super) browser_download_url: String,
}

#[derive(Deserialize)]
pub(super) struct Release {
    tag_name: String,
    pub(super) assets: Vec<ReleaseAsset>,
}

impl Release {
    fn version(&self) -> anyhow::Result<semver::Version> {
        let ver = self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name);
        Ok(semver::Version::parse(ver)?)
    }
}

pub(super) async fn download_with_progress(
    pb: &ProgressBar,
    client: &reqwest::Client,
    url: &str,
) -> Result<http_body_util::Collected<Bytes>, anyhow::Error> {
    let response = client.get(url).send().await?.error_for_status()?;

    let pb_style = pb.style();

    pb.set_style(ProgressStyle::with_template("{spinner} {prefix}{msg} {bytes}/{total_bytes} ({eta})").unwrap());
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

    pb.set_style(pb_style);

    Ok(body)
}

const DOWNLOAD_NAME: &str = if cfg!(windows) {
    concat!("spacetime-", env!("BUILD_TARGET"), ".zip")
} else {
    concat!("spacetime-", env!("BUILD_TARGET"), ".tar.gz")
};
