use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::Context;
use cargo_metadata::Message;
use duct::cmd;

pub(crate) fn build(project_path: &Path) -> anyhow::Result<PathBuf> {
    // Make sure that we have the wasm target installed (ok to run if its already installed)
    cmd!("rustup", "target", "add", "wasm32-unknown-unknown").run()?;
    let reader = cmd!(
        "cargo",
        "--config=net.git-fetch-with-cli=true",
        "build",
        "--target=wasm32-unknown-unknown",
        "--release",
        "--message-format=json-render-diagnostics"
    )
    .dir(project_path)
    .reader()?;
    let mut artifact = None;
    for message in Message::parse_stream(io::BufReader::new(reader)) {
        if let Ok(Message::CompilerArtifact(art)) = message {
            artifact = Some(art);
        } else if let Err(error) = message {
            return Err(anyhow::anyhow!(error));
        }
    }
    let artifact = artifact.context("no artifact found?")?;
    let artifact = artifact.filenames.into_iter().next().context("no wasm?")?;

    let clippy_conf_dir = tempfile::tempdir()?;
    fs::write(clippy_conf_dir.path().join("clippy.toml"), CLIPPY_TOML)?;
    println!("checking crate with spacetimedb's clippy configuration");
    // TODO: should we pass --no-deps here? leaving it out could be valuable if a module is split
    //       into multiple crates, but without it it lints on proc-macro crates too
    let out = cmd!(
        "cargo",
        "--config=net.git-fetch-with-cli=true",
        "clippy",
        "--target=wasm32-unknown-unknown",
        // TODO: pass -q? otherwise it might be too busy
        // "-q",
        "--",
        "--no-deps",
        "-Aclippy::all",
        "-Dclippy::disallowed-macros"
    )
    .dir(project_path)
    .env("CLIPPY_DISABLE_DOCS_LINKS", "1")
    .env("CLIPPY_CONF_DIR", clippy_conf_dir.path())
    .unchecked()
    .run()?;
    anyhow::ensure!(out.status.success(), "clippy found a lint error");

    Ok(artifact.into())
}

const CLIPPY_TOML: &str = r#"
disallowed-macros = [
    { path = "std::print",       reason = "print!() has no effect inside a spacetimedb module; use log::info!() instead" },
    { path = "std::println",   reason = "println!() has no effect inside a spacetimedb module; use log::info!() instead" },
    { path = "std::eprint",     reason = "eprint!() has no effect inside a spacetimedb module; use log::warn!() instead" },
    { path = "std::eprintln", reason = "eprintln!() has no effect inside a spacetimedb module; use log::warn!() instead" },
    { path = "std::dbg",      reason = "std::dbg!() has no effect inside a spacetimedb module; import spacetime's dbg!() macro instead" },
]
"#;
