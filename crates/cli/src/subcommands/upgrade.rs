use std::io::Write;
use std::{env, fs};

extern crate regex;

use crate::version;
use clap::{Arg, ArgMatches};
use flate2::read::GzDecoder;
use futures::stream::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use tar::Archive;

pub fn cli() -> clap::Command {
    clap::Command::new("upgrade")
        .about("Checks for updates for the currently running spacetime CLI tool")
        .arg(Arg::new("version").help("The specific version to upgrade to"))
        .arg(
            Arg::new("force")
                .short('f')
                .long("force")
                .help("If this flag is present, the upgrade will be performed even if the version is the same")
                .action(clap::ArgAction::SetTrue),
        )
        .after_help("Run `spacetime help upgrade` for more detailed information.\n")
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

fn get_download_name() -> String {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    let os_str = match os {
        "macos" => "darwin",
        "windows" => return "spacetime.exe".to_string(),
        "linux" => "linux",
        _ => panic!("Unsupported OS"),
    };

    let arch_str = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => panic!("Unsupported architecture"),
    };

    format!("spacetime.{}-{}.tar.gz", os_str, arch_str)
}

fn clean_version(version: &str) -> Option<String> {
    let re = Regex::new(r"v?(\d+\.\d+\.\d+)").unwrap();
    re.captures(version)
        .and_then(|cap| cap.get(1))
        .map(|match_| match_.as_str().to_string())
}

async fn get_release_tag_from_version(release_version: &str) -> Result<Option<String>, reqwest::Error> {
    let release_version = format!("v{}-beta", release_version);
    let url = "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases";
    let client = reqwest::Client::builder()
        .user_agent(format!("SpacetimeDB CLI/{}", version::CLI_VERSION))
        .build()?;
    let releases: Vec<Value> = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            format!("SpacetimeDB CLI/{}", version::CLI_VERSION).as_str(),
        )
        .send()
        .await?
        .json()
        .await?;

    for release in releases.iter() {
        if let Some(release_tag) = release["tag_name"].as_str() {
            if release_tag.starts_with(&release_version) {
                return Ok(Some(release_tag.to_string()));
            }
        }
    }
    Ok(None)
}

async fn download_with_progress(client: &reqwest::Client, url: &str, temp_path: &Path) -> Result<(), anyhow::Error> {
    let response = client.get(url).send().await?;
    let total_size = match response.headers().get(reqwest::header::CONTENT_LENGTH) {
        Some(size) => size.to_str().unwrap().parse::<u64>().unwrap(),
        None => 0,
    };

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar().template("{spinner} Downloading update... {bytes}/{total_bytes} ({eta})"),
    );

    let mut file = fs::File::create(temp_path)?;
    let mut downloaded_bytes = 0;

    let mut response_stream = response.bytes_stream();
    while let Some(chunk) = response_stream.next().await {
        let chunk = chunk?;
        downloaded_bytes += chunk.len();
        pb.set_position(downloaded_bytes as u64);
        file.write_all(&chunk)?;
    }

    pb.finish_with_message("Download complete.");
    Ok(())
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let version = args.get_one::<String>("version");
    let current_exe_path = env::current_exe()?;
    let force = args.get_flag("force");

    let url = match version {
        None => "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases/latest".to_string(),
        Some(release_version) => {
            let release_tag = get_release_tag_from_version(release_version).await?;
            if release_tag.is_none() {
                return Err(anyhow::anyhow!("No release found for version {}", release_version));
            }
            format!(
                "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases/tags/{}",
                release_tag.unwrap()
            )
        }
    };

    let client = reqwest::Client::builder()
        .user_agent(format!("SpacetimeDB CLI/{}", version::CLI_VERSION))
        .build()?;

    print!("Finding version...");
    std::io::stdout().flush()?;
    let release: Release = client.get(url).send().await?.json().await?;
    let release_version = clean_version(&release.tag_name).unwrap();
    println!("done.");

    if release_version == version::CLI_VERSION {
        println!("You're already running the latest version: {}", version::CLI_VERSION);
        if !force {
            return Ok(());
        } else {
            println!("Force flag is set, continuing with upgrade.");
        }
    }

    let download_name = get_download_name();
    let asset = release.assets.iter().find(|&asset| asset.name == download_name);

    if asset.is_none() {
        return Err(anyhow::anyhow!(
            "No assets available for the detected OS and architecture."
        ));
    }

    println!(
        "You are currently running version {} of spacetime. The version you're upgrading to is {}.",
        version::CLI_VERSION,
        release_version,
    );
    println!(
        "This will replace the current executable at {}.",
        current_exe_path.display()
    );
    print!("Do you want to continue? [y/N] ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" && input.trim().to_lowercase() != "yes" {
        println!("Aborting upgrade.");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?.into_path();
    let temp_path = &temp_dir.join(download_name.clone());
    download_with_progress(&client, &asset.unwrap().browser_download_url, temp_path).await?;

    if download_name.to_lowercase().ends_with(".tar.gz") || download_name.to_lowercase().ends_with("tgz") {
        let tar_gz = fs::File::open(temp_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut spacetime_found = false;
        for mut file in archive.entries()?.filter_map(|e| e.ok()) {
            if let Ok(path) = file.path() {
                if path.ends_with("spacetime") {
                    spacetime_found = true;
                    file.unpack(temp_dir.join("spacetime"))?;
                }
            }
        }

        if !spacetime_found {
            fs::remove_dir_all(&temp_dir)?;
            return Err(anyhow::anyhow!("Spacetime executable not found in archive"));
        }
    }

    let new_exe_path = if temp_path.to_str().unwrap().ends_with(".exe") {
        temp_path.clone()
    } else if download_name.ends_with(".tar.gz") {
        temp_dir.join("spacetime")
    } else {
        fs::remove_dir_all(&temp_dir)?;
        return Err(anyhow::anyhow!("Unsupported download type"));
    };

    // Move the current executable into a temporary directory, which will later be deleted by the OS
    let current_exe_temp_dir = env::temp_dir();
    let current_exe_to_temp = current_exe_temp_dir.join("spacetime_old");
    fs::rename(&current_exe_path, current_exe_to_temp)?;
    fs::rename(new_exe_path, &current_exe_path)?;
    fs::remove_dir_all(&temp_dir)?;

    println!("spacetime has been updated to version {}", release_version);

    Ok(())
}
