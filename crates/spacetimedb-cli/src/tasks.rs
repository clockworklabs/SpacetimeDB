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

pub(crate) fn pre_publish(project_path: &Path, use_cargo: bool) -> anyhow::Result<()> {
    build(project_path)?;

    // Update the running module
    // TODO: just call into crate::subcommands::{identity, energy}
    if use_cargo {
        cmd!("cargo", "run", "identity", "init-default", "--quiet").run()?;
        cmd!("cargo", "run", "energy", "set-balance", "5000000000000000", "--quiet").run()?;
    } else {
        cmd!("spacetime", "identity", "init-default", "--quiet").run()?;
        cmd!("spacetime", "energy", "set-balance", "5000000000000000", "--quiet").run()?;
    }

    Ok(())
}
