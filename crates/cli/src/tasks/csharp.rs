use anyhow::Context;
use duct::cmd;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn build_csharp(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    // Make sure that we have the wasm target installed (ok to run if its already installed)
    cmd!(
        "dotnet",
        "workload",
        "install",
        "wasi-experimental",
        "--skip-manifest-update"
    )
    .run()?;

    let config_name = if build_debug { "Debug" } else { "Release" };

    let mut output_path = project_path.join(format!("bin/{config_name}/net8.0/wasi-wasm/AppBundle/StdbModule.wasm"));

    // Ensure the project path exists.
    fs::metadata(project_path).with_context(|| {
        format!(
            "The provided project path '{}' does not exist.",
            project_path.to_str().unwrap()
        )
    })?;

    // run dotnet publish using cmd macro
    let result = cmd!("dotnet", "publish", "-c", config_name).dir(project_path).run();
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
        // check for the old .NET 7 path for projects that haven't migrated yet
        output_path = project_path.join(format!("bin/{config_name}/net7.0/StdbModule.wasm"));
        if output_path.exists() {
            eprintln!("Warning: Your project is using the old .NET 7.0 WebAssembly bindings.");
            eprintln!("Those will be deprecated in the future. Please migrate your project to the new .NET 8.0 template.");
        } else {
            anyhow::bail!("Built project successfully but couldn't find the output file.");
        }
    }

    Ok(output_path)
}
