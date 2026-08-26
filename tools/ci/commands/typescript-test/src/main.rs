#![allow(clippy::disallowed_macros)]
use anyhow::{bail, ensure, Result};
use ci_common::pnpm;
use clap::Parser;

/// Runs TypeScript workspace tests and template build checks.
#[derive(Parser)]
struct Cli {
    /// Do not build CLI and standalone; use the binaries selected by SPACETIME_BIN.
    #[arg(long)]
    no_build: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.no_build {
        ci_common::require_runtime()?;
    } else {
        ensure!(
            std::env::var_os("SPACETIME_BIN").is_none(),
            "SPACETIME_BIN requires --no-build"
        );
    }

    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
    pnpm(["test"]).dir("crates/bindings-typescript").run()?;
    pnpm(["generate"]).dir("templates/chat-react-ts").run()?;
    let diff_status = duct::cmd!(
        "bash",
        "tools/check-diff.sh",
        "templates/chat-react-ts/src/module_bindings"
    )
    .run()?;
    if !diff_status.status.success() {
        bail!("Bindings are dirty. Please generate bindings again and commit them to this branch.");
    }
    pnpm(["build"]).dir("templates/chat-react-ts").run()?;
    pnpm(["-r", "--filter", "./**", "run", "build"])
        .dir("templates")
        .run()?;
    pnpm(["-r", "--filter", "./**", "run", "build"])
        .dir("crates/bindings-typescript")
        .run()?;
    Ok(())
}
