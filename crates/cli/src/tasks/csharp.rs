use duct::cmd;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn build_csharp(project_path: &Path, _build_debug: bool) -> anyhow::Result<PathBuf> {
    // NOTE: wasi.sdk does not currently optimize release

    let output_path = project_path.join("bin/Release/net7.0/StdbModule.wasm");

    // delete existing wasm file if exists
    if output_path.exists() {
        std::fs::remove_file(&output_path)?;
    }

    // Ensure the project path exists
    if fs::metadata(project_path).is_err() {
        anyhow::bail!(
            "The provided project path '{}' does not exist.",
            project_path.to_str().unwrap()
        );
    }

    // run dotnet publish using cmd macro
    let result = cmd!("dotnet", "publish", "-c", "Release").dir(project_path).run();
    match result {
        Ok(_) => {}
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::bail!("Failed to build project. dotnet not found in path. Please install the .NET Core SDK.");
            } else {
                anyhow::bail!("Failed to build project. {}", error);
            }
        }
    }

    // check if file exists
    if !output_path.exists() {
        anyhow::bail!("Failed to build project");
    }

    Ok(output_path)
}
