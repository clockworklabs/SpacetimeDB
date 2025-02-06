use anyhow::Context;
use std::fs;
use std::path::{Path, PathBuf};

fn parse_major_version(version: &str) -> Option<u8> {
    version.split('.').next()?.parse::<u8>().ok()
}

pub(crate) fn build_csharp(project_path: &Path, build_debug: bool) -> anyhow::Result<PathBuf> {
    // All `dotnet` commands must execute in the project directory, otherwise
    // global.json won't have any effect and wrong .NET SDK might be picked.
    macro_rules! dotnet {
        ($($arg:expr),*) => {
            duct::cmd!("dotnet", $($arg),*).dir(project_path)
        };
    }

    // Check if the `wasi-experimental` workload is installed. Unfortunately, we
    // have to do this by inspecting the human-readable output. There is a
    // hidden `--machine-readable` flag but it also mixes in human-readable
    // output as well as unnecessarily updates various unrelated manifests.
    match dotnet!("workload", "list").read() {
        Ok(workloads) if workloads.contains("wasi-experimental") => {}
        Ok(_) => {
            // If wasi-experimental is not found, first check if we're running
            // on .NET SDK 8.0. We can't even install that workload on older
            // versions, and we don't support .NET 9.0 yet, so this helps to
            // provide a nicer message than "Workload ID wasi-experimental is not recognized.".
            let version = dotnet!("--version").read().unwrap_or_default();
            if parse_major_version(&version) != Some(8) {
                anyhow::bail!(concat!(
                    ".NET SDK 8.0 is required, but found {version}.\n",
                    "If you have multiple versions of .NET SDK installed, configure your project using https://learn.microsoft.com/en-us/dotnet/core/tools/global-json."
                ));
            }

            // Finally, try to install the workload ourselves. On some systems
            // this might require elevated privileges, so print a nice error
            // message if it fails.
            dotnet!(
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
            anyhow::bail!("dotnet not found in PATH. Please install .NET SDK 8.0.")
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
    dotnet!("publish", "-c", config_name, "-v", "quiet").run()?;

    // check if file exists
    let subdir = if std::env::var_os("EXPERIMENTAL_WASM_AOT").is_some_and(|v| v == "1") {
        "publish"
    } else {
        "AppBundle"
    };

    // check for the old .NET 7 path for projects that haven't migrated yet
    let bad_output_paths = [
        project_path.join(format!("bin/{config_name}/net7.0/StdbModule.wasm")),
        // for some reason there is sometimes a tilde here?
        project_path.join(format!("bin~/{config_name}/net7.0/StdbModule.wasm")),
    ];
    if bad_output_paths.iter().any(|p| p.exists()) {
        anyhow::bail!(concat!(
            "Looks like your project is using the deprecated .NET 7.0 WebAssembly bindings.\n",
            "Please migrate your project to the new .NET 8.0 template and delete the folders: bin, bin~, obj, obj~"
        ));
    }
    let possible_output_paths = [
        project_path.join(format!("bin/{config_name}/net8.0/wasi-wasm/{subdir}/StdbModule.wasm")),
        project_path.join(format!("bin~/{config_name}/net8.0/wasi-wasm/{subdir}/StdbModule.wasm")),
    ];
    if possible_output_paths.iter().all(|p| p.exists()) {
        anyhow::bail!(concat!(
            "For some reason, your project has both a `bin` and a `bin~` folder.\n",
            "I don't know which to use, so please delete both and rerun this command so that we can see which is up-to-date."
        ));
    }
    for output_path in possible_output_paths {
        if output_path.exists() {
            return Ok(output_path);
        }
    }
    anyhow::bail!("Built project successfully but couldn't find the output file.");
}
