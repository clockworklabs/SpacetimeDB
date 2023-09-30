use std::process::Command;

// https://stackoverflow.com/questions/43753491/include-git-commit-hash-as-string-into-rust-program
fn main() {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output().unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
