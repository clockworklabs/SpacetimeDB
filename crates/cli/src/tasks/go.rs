use anyhow::Context;
use std::fs;
use std::path::{Path, PathBuf};

fn parse_go_version(version: &str) -> Option<(u8, u8)> {
    // Extract version from "go1.21.0" format
    let version = version.strip_prefix("go")?;
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let major = parts[0].parse::<u8>().ok()?;
        let minor = parts[1].parse::<u8>().ok()?;
        Some((major, minor))
    } else {
        None
    }
}

fn is_go_version_compatible(version: &str) -> bool {
    if let Some((major, minor)) = parse_go_version(version) {
        // Require Go 1.21 or later for WASM compilation
        major > 1 || (major == 1 && minor >= 21)
    } else {
        false
    }
}

pub(crate) fn build_go(project_path: &Path, _lint_dir: Option<&Path>, build_debug: bool) -> anyhow::Result<PathBuf> {
    // All `go` commands must execute in the project directory
    macro_rules! go {
        ($($arg:expr),*) => {
            duct::cmd!("go", $($arg),*).dir(project_path)
        };
    }

    // Check if Go is installed and get version
    let version = match go!("version").read() {
        Ok(output) => {
            // Parse "go version go1.21.0 darwin/arm64" format
            let version_part = output.split_whitespace()
                .nth(2)
                .unwrap_or("")
                .to_string();
            
            if !is_go_version_compatible(&version_part) {
                anyhow::bail!(concat!(
                    "Go 1.21 or later is required for WASM compilation, but found {}.\n",
                    "Please upgrade Go from https://golang.org/dl/"
                ), version_part);
            }
            version_part
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("go not found in PATH. Please install Go 1.21 or later from https://golang.org/dl/")
        }
        Err(error) => anyhow::bail!("Failed to check Go version: {error}"),
    };

    // Ensure the project path exists
    fs::metadata(project_path).with_context(|| {
        format!(
            "The provided project path '{}' does not exist.",
            project_path.to_str().unwrap()
        )
    })?;

    // Ensure go.mod exists
    let go_mod_path = project_path.join("go.mod");
    if !go_mod_path.exists() {
        anyhow::bail!(
            "go.mod not found in project directory. Please run 'go mod init' to initialize a Go module."
        );
    }

    // Set output file name
    let output_name = if build_debug {
        "module-debug.wasm"
    } else {
        "module.wasm"
    };

    eprintln!("Building Go module to WASM with Go {}...", version);

    // Build with appropriate flags for debug vs release
    if build_debug {
        // Debug build: include debug info, disable optimizations
        go!("build", "-o", output_name, "-gcflags=all=-N -l", ".")
            .env("GOOS", "wasip1")
            .env("GOARCH", "wasm")
            .run()
            .context("Go WASM compilation failed")?;
    } else {
        // Release build: optimize for size
        go!("build", "-o", output_name, "-ldflags=-s -w", ".")
            .env("GOOS", "wasip1")
            .env("GOARCH", "wasm")
            .run()
            .context("Go WASM compilation failed")?;
    }

    // Find the output WASM file
    let output_path = project_path.join(output_name);
    if !output_path.exists() {
        anyhow::bail!("Built project successfully but couldn't find the output file: {}", output_name);
    }

    // Report file size
    if let Ok(metadata) = fs::metadata(&output_path) {
        let size_mb = metadata.len() as f64 / 1024.0 / 1024.0;
        eprintln!("WASM module built successfully: {} ({:.1} MB)", output_name, size_mb);
    }

    Ok(output_path)
}

// Optional: Go formatting function for code generation (similar to dotnet_format)
pub(crate) fn go_format(files: impl IntoIterator<Item = PathBuf>) -> anyhow::Result<()> {
    let file_list: Vec<PathBuf> = files.into_iter().collect();
    
    if file_list.is_empty() {
        return Ok(());
    }

    // Use gofmt to format Go files
    let mut args = vec!["-w".to_string()];
    for file in file_list {
        args.push(file.to_string_lossy().to_string());
    }
    
    duct::cmd("gofmt", args).run().context("Failed to format Go files with gofmt")?;
    Ok(())
} 