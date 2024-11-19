use duct::cmd;
use std::io;
use std::path::Path;

/// Find an executable in the `PATH`.
pub(crate) fn find_executable(exe_name: impl AsRef<Path>) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(&exe_name))
            .find(|x| x.is_file())
    })
}

/// Check if `rustup` is installed (aka: Is in the `PATH`).
pub(crate) fn has_rust_up() -> bool {
    match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" | "macos" => find_executable("rustup").is_some(),
        "windows" => find_executable("rustup.exe").is_some(),
        unsupported_os => {
            eprintln!("This OS may be unsupported for `rustup`: {unsupported_os}");
            false
        }
    }
}

/// Check if `rustfmt` is installed (aka: Is in the `PATH`).
pub(crate) fn has_rust_fmt() -> bool {
    match std::env::consts::OS {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "solaris" | "macos" => find_executable("rustfmt").is_some(),
        "windows" => find_executable("rustfmt.exe").is_some(),
        unsupported_os => {
            eprintln!("This OS may be unsupported for `rustfmt`: {unsupported_os}");
            false
        }
    }
}

/// Check if the [Target] is installed.
///
/// **NOTE:** If `rustup` is not installed, we check inside the `rustc sysroot` directory.
pub(crate) fn has_wasm32_target() -> bool {
    let result = || {
        if has_rust_up() {
            let output = cmd!("rustup", "target", "list", "--installed").read()?;
            Ok(output.contains("wasm32-unknown-unknown"))
        } else {
            // When `rustup` is not installed, we need to manually check the [Target] inside the sysroot directory
            let root = cmd!("rustc", "--print", "sysroot").read()?;
            Path::new(&format!("{}/lib/rustlib/{}", root.trim(), "wasm32-unknown-unknown"))
                .try_exists()
                .map_err(|err: io::Error| anyhow::anyhow!(err))
        }
    };

    result().unwrap_or_else(|err| {
        eprintln!("Error checking for wasm32 target: {err}");
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_target() -> anyhow::Result<()> {
        assert!(has_wasm32_target());
        Ok(())
    }
}
