#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::pnpm;
use duct::cmd;
fn main() -> Result<()> {
    env_logger::init();
    pnpm([
        "install",
        "--filter",
        "./crates/bindings-typescript...",
        "--filter",
        "./modules/module-test-ts...",
    ])
    .run()?;
    pnpm(["build"]).dir("crates/bindings-typescript").run()?;
    cmd!("cargo", "test", "-p", "spacetimedb-codegen").run()?;
    // Pre-build the CLI so that it _doesn't_ get `cargo update`d, since that may break the build.
    cmd!("cargo", "build", "-p", "spacetimedb-cli").run()?;
    // Make sure the `Cargo.lock` file reflects the latest available versions.
    // This is what users would end up with on a fresh module, so we want to
    // catch any compile errors arising from a different transitive closure
    // of dependencies than what is in the workspace lock file.
    //
    // For context see also: https://github.com/clockworklabs/SpacetimeDB/pull/2714
    cmd!("cargo", "update").run()?;
    let cli_path = ci_common::repo_root()
        .join("target/debug/spacetimedb-cli")
        .with_extension(std::env::consts::EXE_EXTENSION);
    cmd!(cli_path, "build", "--module-path", "modules/module-test",).run()?;

    Ok(())
}
