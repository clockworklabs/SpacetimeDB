#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use ci_common::pnpm;
use duct::cmd;
fn run_typescript_tests() -> Result<()> {
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
    pnpm(["test"]).dir("crates/bindings-typescript").run()?;
    pnpm(["generate"]).dir("templates/chat-react-ts").run()?;
    let diff_status = cmd!(
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

fn main() -> Result<()> {
    run_typescript_tests()
}
