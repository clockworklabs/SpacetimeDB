use std::path::PathBuf;
use std::process::Command;

// https://stackoverflow.com/questions/43753491/include-git-commit-hash-as-string-into-rust-program
#[allow(clippy::disallowed_macros)]
fn main() {
    track_git_hash_inputs();
    let git_hash = find_git_hash();
    println!("cargo:rustc-env=GIT_HASH={git_hash}");
}

fn git_stdout(args: &[&str]) -> String {
    let output = Command::new("git").args(args).output().unwrap();
    assert!(output.status.success(), "git {} failed", args.join(" "));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn git_path(path: &str) -> PathBuf {
    git_stdout(&["rev-parse", "--path-format=absolute", "--git-path", path]).into()
}

fn track_git_hash_inputs() {
    println!("cargo:rerun-if-env-changed=SPACETIMEDB_NIX_BUILD_GIT_COMMIT");
    if nix_injected_commit_hash().is_some() {
        return;
    }

    println!("cargo:rerun-if-changed={}", git_path("HEAD").display());

    let symbolic_ref = Command::new("git")
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .unwrap();
    if symbolic_ref.status.success() {
        let symbolic_ref = String::from_utf8(symbolic_ref.stdout).unwrap();
        let symbolic_ref_path = git_path(symbolic_ref.trim());
        if symbolic_ref_path.exists() {
            println!("cargo:rerun-if-changed={}", symbolic_ref_path.display());
        }
    }

    let packed_refs = git_path("packed-refs");
    if packed_refs.exists() {
        println!("cargo:rerun-if-changed={}", packed_refs.display());
    }
}

fn nix_injected_commit_hash() -> Option<String> {
    use std::env::VarError;
    // Our flake.nix sets this environment variable to be our git commit hash during the build.
    // This is important because git metadata is otherwise not available within the nix build sandbox,
    // and we don't install the git command-line tool in our build.
    match std::env::var("SPACETIMEDB_NIX_BUILD_GIT_COMMIT") {
        Ok(commit_sha) => {
            // Var is set, we're building under Nix.
            Some(commit_sha)
        }

        Err(VarError::NotPresent) => {
            // Var is not set, we're not in Nix.
            None
        }
        Err(VarError::NotUnicode(gross)) => {
            // Var is set but is invalid unicode, something is very wrong.
            panic!("Injected commit hash is not valid unicode: {gross:?}")
        }
    }
}

fn find_git_hash() -> String {
    nix_injected_commit_hash().unwrap_or_else(|| {
        // When we're *not* building in Nix, we can assume that git metadata is still present in the filesystem,
        // and that the git command-line tool is installed.
        let output = Command::new("git").args(["rev-parse", "HEAD"]).output().unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    })
}
