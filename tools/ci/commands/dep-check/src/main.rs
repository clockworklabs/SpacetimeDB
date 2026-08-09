use anyhow::Result;
use ci_common::ensure_repo_root;
use duct::cmd;

fn main() -> Result<()> {
    ensure_repo_root()?;
    let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        args.push("tools/ci".into());
    }
    cmd("cargo-machete", args).run()?;
    Ok(())
}
