use anyhow::{bail, ensure, Context, Result};
use duct::{cmd, Expression};
use std::env;
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

pub fn require_spacetime_bin() -> Result<PathBuf> {
    let path = env::var_os("SPACETIME_BIN")
        .map(PathBuf::from)
        .context("SPACETIME_BIN is not set")?;
    ensure!(
        path.is_absolute(),
        "SPACETIME_BIN must be an absolute path, got {}",
        path.display()
    );
    ensure!(
        path.is_file(),
        "SpacetimeDB CLI binary does not exist at {}",
        path.display()
    );
    Ok(path)
}

pub fn require_runtime() -> Result<()> {
    let cli = require_spacetime_bin()?;
    let standalone = cli
        .with_file_name("spacetimedb-standalone")
        .with_extension(env::consts::EXE_EXTENSION);
    ensure!(
        standalone.is_file(),
        "SpacetimeDB standalone binary does not exist beside the CLI at {}",
        standalone.display()
    );
    Ok(())
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
