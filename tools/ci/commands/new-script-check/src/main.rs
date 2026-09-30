use anyhow::Result;
use ci_common::ensure_repo_root;
use ci_script_policy::{find_new_scripts, report_violations};
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
    let violations = find_new_scripts(Path::new("."), &args.base_ref, is_allowed)?;
    report_violations(&violations)
}

fn is_allowed(path: &Path) -> bool {
    // Add reviewed exceptions here. Paths are relative to the repository root.
    const ALLOWED_FILES: &[&str] = &[];
    const ALLOWED_DIRS: &[&str] = &[];

    path_is_allowed(path, ALLOWED_FILES, ALLOWED_DIRS)
}

fn path_is_allowed(path: &Path, files: &[&str], dirs: &[&str]) -> bool {
    files.iter().any(|allowed| path == Path::new(allowed)) || dirs.iter().any(|allowed| path.starts_with(allowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exceptions_match_exact_files_and_directory_boundaries() {
        let files = &["tools/allowed.py"];
        let dirs = &["docs/scripts"];
        assert!(path_is_allowed(Path::new("tools/allowed.py"), files, dirs));
        assert!(path_is_allowed(Path::new("docs/scripts/run.sh"), files, dirs));
        assert!(!path_is_allowed(Path::new("tools/allowed.py.bak"), files, dirs));
        assert!(!path_is_allowed(Path::new("docs/scripts-other/run.sh"), files, dirs));
        assert!(!is_allowed(Path::new("tool.py")));
    }
}
