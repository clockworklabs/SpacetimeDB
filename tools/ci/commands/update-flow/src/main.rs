#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use clap::Parser;
use duct::cmd;

#[derive(Parser)]
#[command(
    about = "Tests the update flow",
    long_about = "Tests the update flow\n\nTests the self-update flow by building the spacetimedb-update binary for the specified target, by default the current target, and performing a self-install into a temporary directory."
)]
struct Args {
    #[arg(
        long,
        long_help = "Target triple to build for, by default the current target. Used by github workflows to check the update flow on multiple platforms."
    )]
    target: Option<String>,

    #[arg(
        long,
        default_value = "false",
        long_help = "Whether to enable github token authentication feature when building the update binary. By default this is disabled."
    )]
    github_token_auth: bool,
}

fn main() -> Result<()> {
    let Args {
        target,
        github_token_auth,
    } = Args::parse();
    let mut common_args = vec![];
    if let Some(target) = target.as_ref() {
        common_args.push("--target");
        common_args.push(target);
    }
    if github_token_auth {
        common_args.push("--features");
        common_args.push("github-token-auth");
    }

    cmd(
        "cargo",
        ["build", "-p", "spacetimedb-update"]
            .into_iter()
            .chain(common_args.clone()),
    )
    .run()?;
    // NOTE(bfops): We need the `github-token-auth` feature because we otherwise tend to get ratelimited when we try to fetch `/releases/latest`.
    // My best guess is that, on the GitHub runners, the "anonymous" ratelimit is shared by *all* users of that runner (I think this because it
    // happens very frequently on the `macos-runner`, but we haven't seen it on any others).
    let root_dir = tempfile::tempdir()?;
    let root_dir_string = root_dir.path().to_string_lossy().to_string();
    let root_arg = format!("--root-dir={}", root_dir_string);
    cmd(
        "cargo",
        ["run", "-p", "spacetimedb-update"]
            .into_iter()
            .chain(common_args.clone())
            .chain(["--", "self-install", &root_arg, "--yes"].into_iter()),
    )
    .run()?;

    let mut spacetime_path = root_dir.path().join("spacetime");
    if !std::env::consts::EXE_EXTENSION.is_empty() {
        spacetime_path.set_extension(std::env::consts::EXE_EXTENSION);
    }
    cmd(spacetime_path, [&root_arg, "help"]).run()?;

    Ok(())
}
