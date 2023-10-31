use std::{env, fs};

extern crate regex;

use crate::version;
use clap::{Arg, ArgMatches};
use flate2::read::GzDecoder;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use tar::Archive;

pub fn cli() -> clap::Command {
    clap::Command::new("upgrade")
        .about("Checks for updates for the currently running spacetime CLI tool")
        .arg(Arg::new("version").help("The specific version to upgrade to"))
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

fn get_release_tag_from_version(release_version: &str) -> Result<Option<String>, reqwest::Error> {
    let release_version = format!("v{}-beta", release_version);
    let url = "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases";
    let client = reqwest::blocking::Client::new();
    let releases: Vec<Value> = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            format!("SpacetimeDB CLI/{}", version::CLI_VERSION).as_str(),
        )
        .send()?
        .json()?;

    for release in releases.iter() {
        if let Some(release_tag) = release["tag_name"].as_str() {
            println!("Release: {}", release_tag.clone());
            if release_tag.starts_with(&release_version) {
                return Ok(Some(release_tag.to_string()));
            }
        }
    }
    Ok(None)
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let version = args.get_one::<String>("version");
    let current_exe_path = env::current_exe()?;

    let url = match version {
        None => "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases/latest".to_string(),
        Some(release_version) => {
            let release_tag = get_release_tag_from_version(release_version)?;
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

    let release: Release = client.get(url).send().await?.json().await?;
    let release_version = clean_version(&release.tag_name).unwrap();

    if release_version == version::CLI_VERSION {
        println!("You're already running the latest version: {}", version::CLI_VERSION);
        return Ok(());
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
    println!("Do you want to continue? [y/N]");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" || input.trim().to_lowercase() != "yes" {
        println!("Aborting upgrade.");
        return Ok(());
    }

    // Download the archive from the URL
    let temp_dir = tempfile::tempdir()?.into_path();
    let temp_path = &temp_dir.join(download_name.clone());
    let response = reqwest::blocking::get(&asset.unwrap().browser_download_url)?;
    fs::write(&temp_path, response.bytes()?)?;

    if download_name.ends_with(".tar.gz") {
        let tar_gz = fs::File::open(&temp_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack(&temp_dir)?;
    }

    let new_exe_path = if temp_path.ends_with(".exe") {
        temp_path.clone()
    } else if download_name.ends_with(".tar.gz") {
        temp_dir.join("spacetime")
    } else {
        return Err(anyhow::anyhow!("Unsupported download type"));
    };

    fs::copy(&new_exe_path, current_exe_path)?;

    fs::remove_file(&temp_path)?;
    if download_name.ends_with(".tar.gz") {
        fs::remove_file(new_exe_path)?;
    }

    println!("spacetime has been updated!");

    Ok(())
}
