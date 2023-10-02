use std::{env, fs};

use crate::version;
use clap::{Arg, ArgMatches};
use flate2::read::GzDecoder;
use serde::Deserialize;
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

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let version = args.get_one::<String>("version");
    let current_exe_path = env::current_exe()?;

    let url = match version {
        None => "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases/latest".to_string(),
        Some(version) => format!(
            "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases/tags/{}",
            version
        ),
    };
    let release: Release = reqwest::blocking::get(url)?.json()?;

    if release.tag_name == version::CLI_VERSION {
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

    // Download the archive from the URL
    let temp_path = format!("/tmp/{}", download_name);
    let response = reqwest::blocking::get(&asset.unwrap().browser_download_url)?;
    fs::write(&temp_path, response.bytes()?)?;

    if download_name.ends_with(".tar.gz") {
        let tar_gz = fs::File::open(&temp_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack("/tmp/")?;
    }

    let new_exe_path = if download_name.ends_with(".tar.gz") {
        "/tmp/spacetime".to_string()
    } else {
        temp_path.clone()
    };

    fs::copy(&new_exe_path, current_exe_path)?;

    fs::remove_file(&temp_path)?;
    if download_name.ends_with(".tar.gz") {
        fs::remove_file(new_exe_path)?;
    }

    println!("spacetime has been updated!");

    Ok(())
}
