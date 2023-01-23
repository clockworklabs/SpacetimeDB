use std::io;
use std::path::{Path, PathBuf};

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
    Ok(artifact.filenames.into_iter().next().context("no wasm?")?.into())
}
