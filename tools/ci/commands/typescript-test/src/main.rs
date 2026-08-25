#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use ci_common::pnpm;
use clap::Parser;
use duct::Expression;

/// Runs TypeScript workspace tests and template build checks.
#[derive(Parser)]
struct Cli {
    /// Use release CLI and standalone binaries already present in the Cargo target directory.
    #[arg(long)]
    prebuilt_runtime: bool,
}

fn with_runtime(command: Expression, runtime: Option<&ci_common::PrebuiltRuntime>) -> Expression {
    match runtime {
        Some(runtime) => command.env("SPACETIME_BIN", &runtime.cli),
        None => command,
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let prebuilt_runtime = cli
        .prebuilt_runtime
        .then(ci_common::require_prebuilt_runtime)
        .transpose()?;

    with_runtime(
        pnpm(["build"]).dir("crates/bindings-typescript"),
        prebuilt_runtime.as_ref(),
    )
    .run()?;
    with_runtime(
        pnpm(["test"]).dir("crates/bindings-typescript"),
        prebuilt_runtime.as_ref(),
    )
    .run()?;
    with_runtime(
        pnpm(["generate"]).dir("templates/chat-react-ts"),
        prebuilt_runtime.as_ref(),
    )
    .run()?;
    let diff_status = duct::cmd!(
        "bash",
        "tools/check-diff.sh",
        "templates/chat-react-ts/src/module_bindings"
    )
    .run()?;
    if !diff_status.status.success() {
        bail!("Bindings are dirty. Please generate bindings again and commit them to this branch.");
    }
    with_runtime(
        pnpm(["build"]).dir("templates/chat-react-ts"),
        prebuilt_runtime.as_ref(),
    )
    .run()?;
    with_runtime(
        pnpm(["-r", "--filter", "./**", "run", "build"]).dir("templates"),
        prebuilt_runtime.as_ref(),
    )
    .run()?;
    with_runtime(
        pnpm(["-r", "--filter", "./**", "run", "build"]).dir("crates/bindings-typescript"),
        prebuilt_runtime.as_ref(),
    )
    .run()?;
    Ok(())
}
