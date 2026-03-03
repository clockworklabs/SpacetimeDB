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

    // Verify stdb-gen is available
    let stdb_gen_found = find_executable("stdb-gen").is_some();
    if !stdb_gen_found {
        return Err(anyhow!(
            "`stdb-gen` not found in PATH. \
             Install it with: go install go.digitalxero.dev/stdb-gen@latest"
        ));
    }

    // Run stdb-gen upgrade to ensure latest codegen templates
    eprintln!("Running stdb-gen upgrade...");
    duct::cmd!("stdb-gen", "upgrade")
        .dir(project_path)
        .run()
        .with_context(|| "Failed to run stdb-gen upgrade.")?;

    // Run go generate to produce stdb_generated.go (code generation step)
    eprintln!("Running go generate...");
    duct::cmd!("go", "generate", "./...")
        .dir(project_path)
        .run()
        .with_context(|| "Failed to run go generate. Ensure stdb-gen is available.")?;

    // Build with standard Go compiler
    // GOOS=wasip1 GOARCH=wasm produces WASI Preview 1 compatible WASM
    // -buildmode=c-shared produces a reactor module (exports _initialize, keeps runtime alive)
    // For release: strip debug info with -ldflags "-s -w"
    eprintln!("Building Go module...");

    let mut build_args = vec![
        "build".to_string(),
        "-trimpath".to_string(),
        "-tags=netgo,osusergo".to_string(),
        "-buildmode=c-shared".to_string(),
    ];

    if !build_debug {
        build_args.push("-ldflags=-s -w -extldflags -static".to_string());
    }

    build_args.push("-o".to_string());
    build_args.push(output_path.to_str().unwrap().to_string());
    build_args.push(".".to_string());

    let mut cmd = duct::cmd("go", &build_args)
        .env("GOOS", "wasip1")
        .env("GOARCH", "wasm")
        .env("GODEBUG", "asyncpreemptoff=1")
        .dir(project_path);

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
