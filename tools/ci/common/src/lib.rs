use anyhow::{bail, Result};
use duct::{cmd, Expression};
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

pub fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("failed to find repo root")
        .to_path_buf()
}

pub fn pnpm<I, S>(args: I) -> Expression
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<std::ffi::OsString> = args.into_iter().map(|a| a.as_ref().to_os_string()).collect();
    if cfg!(windows) {
        let mut full: Vec<std::ffi::OsString> = vec!["/c".into(), "pnpm".into()];
        full.extend(args);
        cmd("cmd", full)
    } else {
        cmd("pnpm", args)
    }
}
