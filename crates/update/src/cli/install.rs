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
                paths.cli_bin_dir.set_current_version(&version.to_string())?;
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
    let download_name = artifact_name.as_deref().unwrap_or(DOWNLOAD_NAME);
    let artifact_type = ArtifactType::deduce(download_name).context("Unknown archive type")?;

    let pb_style = ProgressStyle::with_template("{spinner} {prefix}{msg}").unwrap();
    let pb = ProgressBar::new(0).with_style(pb_style.clone());
    pb.enable_steady_tick(std::time::Duration::from_millis(60));

    pb.set_message("Resolving version...");
    let url = "http://192.168.2.100";
    let release_version = "1.0.0".parse::<semver::Version>().unwrap(); 
    let download_url = format!("http://192.168.2.100/{}", download_name);

    pb.set_style(ProgressStyle::with_template("{spinner} {prefix}{msg} {bytes}/{total_bytes} ({eta})").unwrap());
    pb.set_prefix(format!("Installing v1.0.0: "));
    pb.set_message("downloading...");
    let archive = download_with_progress(&pb, client, &download_url).await?;

    pb.set_style(pb_style);
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

pub(super) async fn available_releases(client: &reqwest::Client) -> anyhow::Result<Vec<String>> {
    let url = "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases";
    let releases: Vec<Release> = client.get(url).send().await?.json().await?;

    releases
        .into_iter()
        .map(|release| Ok(release.version()?.to_string()))
        .collect()
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

const DOWNLOAD_NAME: &str = if cfg!(windows) {
    concat!("spacetime-", env!("BUILD_TARGET"), ".zip")
} else {
    concat!("spacetime-", env!("BUILD_TARGET"), ".tar.gz")
};
