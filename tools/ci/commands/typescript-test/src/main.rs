#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use ci_common::pnpm;
use clap::Parser;

/// Runs TypeScript workspace tests and template build checks.
#[derive(Parser)]
struct Cli {}

fn main() -> Result<()> {
    Cli::parse();
    if std::env::var_os("SPACETIME_BIN").is_some() {
        ci_common::require_runtime()?;
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
    // The SDK name conformance test (tools/sdk-names) type-checks names in the test app's
    // bindings, so they must match current codegen. Version bumps don't regenerate them, so the
    // lines that only record the CLI version are ignored.
    pnpm(["generate"]).dir("crates/bindings-typescript/test-app").run()?;
    let test_app_bindings = "crates/bindings-typescript/test-app/src/module_bindings";
    let test_app_diff = duct::cmd!(
        "git",
        "diff",
        "--exit-code",
        "--ignore-matching-lines=^// This was generated using spacetimedb cli version.*",
        "--ignore-matching-lines=^ *cliVersion: '.*' as const,$",
        "--",
        test_app_bindings
    )
    .unchecked()
    .run()?;
    // `git diff` ignores untracked files, so a generated file missing from the commit would pass it.
    let untracked = duct::cmd!(
        "git",
        "ls-files",
        "--others",
        "--exclude-standard",
        "--",
        test_app_bindings
    )
    .read()?;
    if !untracked.is_empty() {
        println!("Generated files that are not committed:\n{untracked}");
    }
    if !test_app_diff.status.success() || !untracked.is_empty() {
        bail!("The test app's bindings are dirty. Run `pnpm generate` in crates/bindings-typescript/test-app and commit them.");
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
