use anyhow::{anyhow, Context};
use std::fs;
use std::path::{Path, PathBuf};

use crate::detect::find_executable;

pub(crate) fn build_go(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    // Verify go is available
    let go_found = find_executable("go").is_some();
    if !go_found {
        return Err(anyhow!(
            "`go` not found in PATH. \
             Install Go 1.24+ from https://go.dev/dl/"
        ));
    }

    // Ensure the project path exists
    fs::metadata(project_path).with_context(|| {
        format!(
            "The provided project path '{}' does not exist.",
            project_path.display()
        )
    })?;

    // Ensure go.mod exists
    if !project_path.join("go.mod").exists() {
        return Err(anyhow!(
            "No go.mod found in '{}'. Is this a Go module?",
            project_path.display()
        ));
    }

    // Create output directory
    let target_dir = project_path.join("target");
    fs::create_dir_all(&target_dir).with_context(|| {
        format!("Failed to create target directory '{}'", target_dir.display())
    })?;

    let output_path = target_dir.join("module.wasm");

    // Build with standard Go compiler
    // GOOS=wasip1 GOARCH=wasm produces WASI Preview 1 compatible WASM
    // -buildmode=c-shared produces a reactor module (exports _initialize, keeps runtime alive)
    // For release: strip debug info with -ldflags "-s -w"
    eprintln!("Building Go module...");

    let mut cmd = duct::cmd!(
        "go",
        "build",
        "-buildmode=c-shared",
        "-o",
        output_path.to_str().unwrap(),
        "."
    )
    .env("GOOS", "wasip1")
    .env("GOARCH", "wasm")
    .dir(project_path);

    if !build_debug {
        cmd = cmd.env("GOFLAGS", "-ldflags=-s -w");
    }

    cmd.run()
        .with_context(|| "Failed to build Go module")?;

    if !output_path.exists() {
        return Err(anyhow!(
            "Go build succeeded but output file '{}' not found.",
            output_path.display()
        ));
    }

    Ok(output_path)
}

pub(crate) fn gofmt(files: impl IntoIterator<Item = PathBuf>) -> anyhow::Result<()> {
    let files: Vec<PathBuf> = files.into_iter().collect();
    if files.is_empty() {
        return Ok(());
    }

    let gofmt_found = find_executable("gofmt").is_some();
    if !gofmt_found {
        eprintln!("Warning: `gofmt` not found in PATH, skipping formatting of generated Go files.");
        return Ok(());
    }

    for file in &files {
        duct::cmd!("gofmt", "-w", file.to_str().unwrap())
            .run()
            .with_context(|| format!("Failed to format '{}'", file.display()))?;
    }

    Ok(())
}
