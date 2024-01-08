use anyhow::Context;
use duct::cmd;
use std::fs;
use std::path::{Path, PathBuf};

fn parse_major_version(version: &str) -> Option<u8> {
    version.split('.').next()?.parse::<u8>().ok()
}

pub(crate) fn build_csharp(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    // Check if the `wasi-experimental` workload is installed. Unfortunately, we
    // have to do this by inspecting the human-readable output. There is a
    // hidden `--machine-readable` flag but it also mixes in human-readable
    // output as well as unnecessarily updates various unrelated manifests.
    match cmd!("dotnet", "workload", "list").read() {
        Ok(workloads) if workloads.contains("wasi-experimental") => {}
        Ok(_) => {
            // If wasi-experimental is not found, first check if we're running
            // on .NET 8.0. We can't even install that workload on older
            // versions, so this helps to provide a nicer message than "Workload
            // ID wasi-experimental is not recognized.".
            let version = cmd!("dotnet", "--version").read().unwrap_or_default();
            if parse_major_version(&version) < Some(8) {
                anyhow::bail!(".NET 8.0 is required, but found {version}.");
            }

            // Finally, try to install the workload ourselves. On some systems
            // this might require elevated privileges, so print a nice error
            // message if it fails.
            cmd!(
                "dotnet",
                "workload",
                "install",
                "wasi-experimental",
                "--skip-manifest-update"
            )
            .run()
            .context(concat!(
                "Couldn't install the required wasi-experimental workload.\n",
                "You might need to install it manually by running `dotnet workload install wasi-experimental` with privileged rights."
            ))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("dotnet not found in PATH. Please install .NET 8.0.")
        }
        Err(error) => anyhow::bail!("{error}"),
    };

    let config_name = if build_debug { "Debug" } else { "Release" };

    // Ensure the project path exists.
    fs::metadata(project_path).with_context(|| {
        format!(
            "The provided project path '{}' does not exist.",
            project_path.to_str().unwrap()
        )
    })?;

    // run dotnet publish using cmd macro
    cmd!("dotnet", "publish", "-c", config_name, "-v", "quiet")
        .dir(project_path)
        .run()?;

    // check if file exists
    let mut output_path = project_path.join(format!("bin/{config_name}/net8.0/wasi-wasm/AppBundle/StdbModule.wasm"));
    if !output_path.exists() {
        // check for the old .NET 7 path for projects that haven't migrated yet
        output_path = project_path.join(format!("bin/{config_name}/net7.0/StdbModule.wasm"));
        if output_path.exists() {
            anyhow::bail!(concat!(
                "Looks like your project is using the deprecated .NET 7.0 WebAssembly bindings.\n",
                "Please migrate your project to the new .NET 8.0 template."
            ));
        } else {
            anyhow::bail!("Built project successfully but couldn't find the output file.");
        }
    }

    Ok(output_path)
}
