use duct::cmd;
use std::path::{Path, PathBuf};

pub(crate) fn build_csharp(project_path: &Path, _build_debug: bool) -> anyhow::Result<PathBuf> {
    // NOTE: wasi.sdk does not currently optimize release

    let output_path = project_path.join("bin/Release/net7.0/StdbModule.wasm");

    // delete existing wasm file if exists
    if output_path.exists() {
        std::fs::remove_file(&output_path)?;
    }

    // run dotnet publish using cmd macro
    cmd!("dotnet", "publish", "-c", "Release").dir(project_path).run()?;

    println!("publish complete at {:?}", output_path);

    // check if file exists
    if !output_path.exists() {
        anyhow::bail!("Failed to build project");
    }

    return Ok(PathBuf::from(output_path));
}
