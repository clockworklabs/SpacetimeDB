use duct::cmd;
use std::io;
use std::path::Path;

/// Find an executable in the `PATH`.
pub(crate) fn find_executable(exe_name: impl AsRef<Path>) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .filter_map(|dir| {
                let full_path = dir.join(&exe_name);
                full_path.is_file().then_some(full_path)
            })
            .next()
    })
}

/// Check if `rustup` is installed (aka: Is in the `PATH`).
pub(crate) fn has_rust_up() -> anyhow::Result<bool> {
    Ok(match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" | "macos" => find_executable("rustup").is_some(),
        "windows" => find_executable("rustup.exe").is_some(),
        unsupported_os => {
            return Err(anyhow::anyhow!(
                "This OS may be unsupported for `rustup`: {unsupported_os}"
            ));
        }
    })
}

/// Check if `rustfmt` is installed (aka: Is in the `PATH`).
pub(crate) fn has_rust_fmt() -> anyhow::Result<bool> {
    Ok(match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" | "macos" => find_executable("rustfmt").is_some(),
        "windows" => find_executable("rustfmt.exe").is_some(),
        unsupported_os => {
            return Err(anyhow::anyhow!(
                "This OS may be unsupported for `rustfmt`: {unsupported_os}"
            ));
        }
    })
}

/// Check if the [Target] is installed.
///
/// **NOTE:** If `rustup` is not installed, we check inside the `rustc sysroot` directory.
pub(crate) fn has_wasm32_target() -> anyhow::Result<bool> {
    if has_rust_up()? {
        let output = cmd!("rustup", "target", "list", "--installed").read()?;
        Ok(output.contains("wasm32-unknown-unknown"))
    } else {
        // When `rustup` is not installed, we need to manually check the [Target] inside the sysroot directory
        let root = cmd!("rustc", "--print", "sysroot").read()?;
        Path::new(&format!("{}/lib/rustlib/{}", root.trim(), "wasm32-unknown-unknown"))
            .try_exists()
            .map_err(|err: io::Error| anyhow::anyhow!(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_target() -> anyhow::Result<()> {
        assert!(has_wasm32_target()?);
        Ok(())
    }
}
