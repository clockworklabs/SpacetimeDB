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

/// Check if the target `wasm32-unknown-unknown` is installed.
pub(crate) fn has_wasm32_target() -> bool {
    let result = || {
        let path = cmd!(
            "rustc",
            "--print",
            "target-libdir",
            "--target",
            "wasm32-unknown-unknown"
        )
        .read()?;
        Path::new(path.trim())
            .try_exists()
            .map_err(|err: io::Error| anyhow::anyhow!(err))
    };

    result().unwrap_or_else(|err| {
        eprintln!("Error checking for wasm32 target: {err}");
        false
    })
}

/// Check if a given `PackageManager` executable is available on `PATH`.
///
/// On Windows, npm/pnpm/yarn are `.cmd` shims while bun is a `.exe`,
/// so we check the platform-appropriate extension.
pub(crate) fn has_package_manager(pm: crate::spacetime_config::PackageManager) -> bool {
    let name = pm.to_string();
    if cfg!(windows) {
        // bun ships as bun.exe; npm, pnpm, yarn are .cmd shims
        let ext = if name == "bun" { "exe" } else { "cmd" };
        find_executable(format!("{name}.{ext}")).is_some()
    } else {
        find_executable(&name).is_some()
    }
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
