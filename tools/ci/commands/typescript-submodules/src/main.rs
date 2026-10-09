use anyhow::Result;
use ci_common::pnpm;
use clap::Parser;

/// Checks TypeScript submodules and builds their packages and examples.
#[derive(Parser)]
struct Cli {}

fn main() -> Result<()> {
    Cli::parse();
    ci_common::ensure_repo_root()?;

    pnpm(["--dir", "crates/bindings-typescript", "run", "build"]).run()?;
    for script in ["lint", "typecheck", "test"] {
        pnpm(["-r", "-F", "./spacetime-*-ts/**", "run", script]).run()?;
    }

    // The recovery test starts and stops its own server on a free local port.
    pnpm(["--dir", "spacetime-cron-ts", "run", "test:recovery"]).run()?;
    pnpm(["-r", "-F", "./spacetime-*-ts/**", "run", "build"]).run()?;
    Ok(())
}
