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
        pnpm(["-r", "-F", "./modules/spacetime-*-ts/**", "run", script]).run()?;
    }

    // The recovery test starts and stops its own server on a free local port.
    pnpm(["--dir", "modules/spacetime-cron-ts", "run", "test:recovery"]).run()?;
    pnpm(["--dir", "modules/spacetime-s3-ts", "run", "test:local"]).run()?;
    pnpm(["-r", "-F", "./modules/spacetime-*-ts/**", "run", "build"]).run()?;
    Ok(())
}
