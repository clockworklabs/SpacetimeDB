use anyhow::Result;
use ci_common::ensure_repo_root;
use ci_script_policy::reject_new_scripts;
use clap::Parser;
use std::path::Path;

#[derive(Parser)]
#[command(about = "Rejects newly introduced Bash and Python scripts.")]
struct Cli {
    /// Git ref to compare against, usually origin/<pull request base branch>.
    #[arg(long)]
    base_ref: String,
}

fn main() -> Result<()> {
    let args = Cli::parse();
    ensure_repo_root()?;
    reject_new_scripts(Path::new("."), &args.base_ref, is_allowed)
}

fn is_allowed(path: &Path) -> bool {
    // StackBench's reference apps provide the `start.sh` every app it grades must have.
    path.starts_with("tools/stack-bench/reference-apps") && path.file_name() == Some("start.sh".as_ref())
}
