use anyhow::{bail, ensure, Result};
use duct::{cmd, Expression};
use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

pub struct PrebuiltRuntime {
    pub cli: PathBuf,
    pub standalone: PathBuf,
}

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

pub fn require_prebuilt_runtime() -> Result<PrebuiltRuntime> {
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("target"));
    let release_dir = target_dir.join("release");
    let binary_path = |name: &str| release_dir.join(name).with_extension(env::consts::EXE_EXTENSION);
    let runtime = PrebuiltRuntime {
        cli: binary_path("spacetimedb-cli"),
        standalone: binary_path("spacetimedb-standalone"),
    };

    for (name, path) in [
        ("spacetimedb-cli", &runtime.cli),
        ("spacetimedb-standalone", &runtime.standalone),
    ] {
        ensure!(
            path.is_file(),
            "--prebuilt-runtime requires {name} at {}",
            path.display()
        );
    }

    Ok(runtime)
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
