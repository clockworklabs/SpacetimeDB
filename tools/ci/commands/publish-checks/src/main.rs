#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use duct::cmd;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        println!("Usage: cargo ci publish-checks");
        return Ok(());
    }
    if !args.is_empty() {
        bail!("cargo ci publish-checks does not accept arguments");
    }

    cmd!("bash", "-lc", "test -d venv || python3 -m venv venv").run()?;
    cmd!("venv/bin/pip3", "install", "argparse", "toml").run()?;

    let crates = cmd!(
        "venv/bin/python3",
        "tools/find-publish-list.py",
        "--recursive",
        "--directories",
        "--quiet",
        "spacetimedb",
        "spacetimedb-sdk"
    )
    .read()?;

    let mut failed = Vec::new();
    for crate_dir in crates.split_whitespace() {
        if let Err(err) = cmd!("venv/bin/python3", "tools/crate-publish-checks.py", crate_dir).run() {
            eprintln!("crate publish checks failed for {crate_dir}: {err}");
            failed.push(crate_dir.to_string());
        }
    }

    if !failed.is_empty() {
        bail!("crate publish checks failed for: {}", failed.join(", "));
    }

    Ok(())
}
