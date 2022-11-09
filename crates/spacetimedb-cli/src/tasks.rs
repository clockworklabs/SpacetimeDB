use std::path::Path;

use duct::cmd;

pub(crate) fn build(project_path: &Path) -> anyhow::Result<()> {
    // Make sure that we have the wasm target installed (ok to run if its already installed)
    cmd!("rustup", "target", "add", "wasm32-unknown-unknown").run()?;
    cmd!(
        "cargo",
        "--config=net.git-fetch-with-cli=true",
        "build",
        "--target=wasm32-unknown-unknown",
        "--release"
    )
    .dir(project_path)
    .run()?;
    Ok(())
}

pub(crate) fn pre_publish(project_path: &Path) -> anyhow::Result<()> {
    build(project_path)?;

    // Update the running module
    // TODO: just call into crate::subcommands::{identity, energy}
    cmd!("spacetime", "identity", "init-default", "--quiet")
        .dir(project_path)
        .run()?;
    cmd!("spacetime", "energy", "set-balance", "5000000000000000", "--quiet")
        .dir(project_path)
        .run()?;

    Ok(())
}
