use crate::bench::utils::{sanitize_db_name, server_name};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/* -------------------------------------------------------------------------- */
/* Shared                                                                     */
/* -------------------------------------------------------------------------- */

pub trait Publisher: Send + Sync {
    fn publish(&self, source: &Path, module_name: &str) -> Result<()>;
}

fn run(cmd: &mut Command, label: &str) -> Result<()> {
    eprintln!("==> {label}: {:?}", cmd);
    let out = cmd.output().with_context(|| format!("{label}: spawn"))?;
    if !out.status.success() {
        let code = out.status.code().unwrap_or(-1);
        bail!(
            "{label} failed (exit={code})\n--- stderr ---\n{}\n--- stdout ---\n{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        );
    }
    Ok(())
}

/* -------------------------------------------------------------------------- */
/* C# Publisher                                                               */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy)]
pub struct DotnetPublisher;

impl DotnetPublisher {
    fn ensure_csproj(root: &Path) -> Result<()> {
        let mut has = false;
        for ent in fs::read_dir(root)? {
            let ent = ent?;
            if ent.path().extension().map(|e| e == "csproj").unwrap_or(false) {
                has = true;
                break;
            }
        }
        if !has {
            bail!("expected a C# project in {}", root.display());
        }
        Ok(())
    }
}

impl Publisher for DotnetPublisher {
    fn publish(&self, source: &Path, module_name: &str) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish csharp module {}", module_name);

        Self::ensure_csproj(source)?;

        let srv = server_name();
        let db = sanitize_db_name(module_name);

        let mut cmd = Command::new("spacetime");
        cmd.arg("build")
            .current_dir(source)
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .env("DOTNET_NOLOGO", "1");
        run(&mut cmd, "spacetime build (csharp)")?;

        let mut pubcmd = Command::new("spacetime");
        pubcmd
            .arg("publish")
            .arg("-c")
            .arg("-y")
            .arg("--server")
            .arg(&srv)
            .arg(&db)
            .current_dir(source);
        run(&mut pubcmd, "spacetime publish (csharp)")?;

        Ok(())
    }
}
/* -------------------------------------------------------------------------- */
/* Rust Publisher                                                             */
/* -------------------------------------------------------------------------- */

#[derive(Clone, Copy)]
pub struct SpacetimeRustPublisher;

impl SpacetimeRustPublisher {
    fn ensure_standalone_manifest(dst: &Path) -> Result<()> {
        if !dst.join("Cargo.toml").exists() {
            bail!("no Cargo.toml in {}", dst.display());
        }
        Ok(())
    }
}

impl Publisher for SpacetimeRustPublisher {
    fn publish(&self, source: &Path, module_name: &str) -> Result<()> {
        if !source.exists() {
            bail!("no source: {}", source.display());
        }
        println!("publish rust module {}", module_name);

        // Build/publish directly from `source`
        Self::ensure_standalone_manifest(source)?;

        // sanitize db + server
        let srv = server_name();
        let db = sanitize_db_name(module_name);

        // 2) Publish
        run(
            Command::new("spacetime")
                .arg("publish")
                .arg("-c")
                .arg("-y")
                .arg("--server")
                .arg(&srv)
                .arg(&db)
                .current_dir(source),
            "spacetime publish",
        )?;

        Ok(())
    }
}
