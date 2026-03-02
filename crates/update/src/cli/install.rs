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

const MIRROR_BASE_URL: &str = "https://spacetimedb-client-binaries.nyc3.digitaloceanspaces.com";

pub(super) fn mirror_asset_url(version: &semver::Version, asset_name: &str) -> String {
    format!("{MIRROR_BASE_URL}/refs/tags/v{version}/{asset_name}")
}

async fn mirror_release(
    client: &reqwest::Client,
    version: Option<&semver::Version>,
    download_name: &str,
) -> anyhow::Result<(semver::Version, Release)> {
    let tag = match version {
        Some(v) => format!("v{v}"),
        None => {
            let url = format!("{MIRROR_BASE_URL}/latest-version");
            client
                .get(&url)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?
                .trim()
                .to_owned()
        }
    };
    let ver_str = tag.strip_prefix('v').unwrap_or(&tag);
    let release_version =
        semver::Version::parse(ver_str).with_context(|| format!("Could not parse version from mirror: {tag}"))?;
    let release = Release {
        tag_name: tag.clone(),
        assets: vec![ReleaseAsset {
            name: download_name.to_owned(),
            browser_download_url: mirror_asset_url(&release_version, download_name),
        }],
    };
    Ok((release_version, release))
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

    // Try GitHub first, fall back to mirror if unavailable.
    let github_result = async {
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
        let release_version = match &version {
            Some(version) => version.clone(),
            None => release.version().context("Could not parse version number")?,
        };
        anyhow::Ok((release_version, release))
    }
    .await;

    let (release_version, release) = match github_result {
        Ok(result) => result,
        Err(github_err) => {
            pb.set_message("GitHub unavailable, trying mirror...");
            mirror_release(client, version.as_ref(), download_name)
                .await
                .map_err(|mirror_err| {
                    anyhow::anyhow!("GitHub failed: {github_err:#}\nMirror also failed: {mirror_err:#}")
                })?
        }
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
    let mirror_url = mirror_asset_url(&release_version, download_name);
    let archive = match download_with_progress(&pb, client, &asset.browser_download_url).await {
        Ok(archive) => archive,
        Err(primary_err) => {
            if asset.browser_download_url == mirror_url {
                return Err(primary_err);
            }
            pb.set_message("download failed, trying mirror...");
            download_with_progress(&pb, client, &mirror_url)
                .await
                .map_err(|mirror_err| {
                    anyhow::anyhow!("Primary download failed: {primary_err:#}\nMirror also failed: {mirror_err:#}")
                })?
        }
    };

    pb.set_message("unpacking...");

    let version_dir = paths.cli_bin_dir.version_dir(&release_version.to_string());
    version_dir.create()?;
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
    match async {
        anyhow::Ok(
            client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json::<Vec<Release>>()
                .await?,
        )
    }
    .await
    {
        Ok(releases) => releases
            .into_iter()
            .map(|release| Ok(release.version()?.to_string()))
            .collect(),
        Err(_) => {
            eprintln!("GitHub unavailable, fetching latest version from mirror...");
            let url = format!("{MIRROR_BASE_URL}/latest-version");
            let tag = client.get(&url).send().await?.error_for_status()?.text().await?;
            let ver_str = tag.trim();
            let ver_str = ver_str.strip_prefix('v').unwrap_or(ver_str);
            semver::Version::parse(ver_str)
                .with_context(|| format!("Could not parse version from mirror: {ver_str}"))?;
            Ok(vec![ver_str.to_owned()])
        }
    }
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
