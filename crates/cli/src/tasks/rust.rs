use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::Context;
use cargo_metadata::Message;
use duct::cmd;

pub(crate) fn build_rust(project_path: &Path, skip_clippy: bool, build_debug: bool) -> anyhow::Result<PathBuf> {
    // Make sure that we have the wasm target installed (ok to run if its already installed)
    cmd!("rustup", "target", "add", "wasm32-unknown-unknown").run()?;
    let reader = if build_debug {
        cmd!(
            "cargo",
            "--config=net.git-fetch-with-cli=true",
            "build",
            "--target=wasm32-unknown-unknown",
            "--message-format=json-render-diagnostics"
        )
    } else {
        cmd!(
            "cargo",
            "--config=net.git-fetch-with-cli=true",
            "build",
            "--target=wasm32-unknown-unknown",
            "--release",
            "--message-format=json-render-diagnostics"
        )
    }
    .dir(project_path)
    .reader()?;

    let mut artifact = None;
    for message in Message::parse_stream(io::BufReader::new(reader)) {
        if let Ok(Message::CompilerArtifact(art)) = message {
            artifact = Some(art);
        } else if let Err(error) = message {
            return Err(anyhow::anyhow!(error));
        }
    }
    let artifact = artifact.context("no artifact found?")?;
    let artifact = artifact.filenames.into_iter().next().context("no wasm?")?;

    if !skip_clippy {
        let clippy_conf_dir = tempfile::tempdir()?;
        fs::write(clippy_conf_dir.path().join("clippy.toml"), CLIPPY_TOML)?;
        println!("checking crate with spacetimedb's clippy configuration");
        // TODO: should we pass --no-deps here? leaving it out could be valuable if a module is split
        //       into multiple crates, but without it it lints on proc-macro crates too
        let out = cmd!(
            "cargo",
            "--config=net.git-fetch-with-cli=true",
            "clippy",
            "--target=wasm32-unknown-unknown",
            // TODO: pass -q? otherwise it might be too busy
            // "-q",
            "--",
            "--no-deps",
            "-Aclippy::all",
            "-Dclippy::disallowed-macros"
        )
        .dir(project_path)
        .env("CLIPPY_DISABLE_DOCS_LINKS", "1")
        .env("CLIPPY_CONF_DIR", clippy_conf_dir.path())
        .unchecked()
        .run()?;
        anyhow::ensure!(out.status.success(), "clippy found a lint error");
    }

    check_for_wasm_bindgen(artifact.as_ref())?;

    Ok(artifact.into())
}

const CLIPPY_TOML: &str = r#"
disallowed-macros = [
    { path = "std::print",       reason = "print!() has no effect inside a spacetimedb module; use log::info!() instead" },
    { path = "std::println",   reason = "println!() has no effect inside a spacetimedb module; use log::info!() instead" },
    { path = "std::eprint",     reason = "eprint!() has no effect inside a spacetimedb module; use log::warn!() instead" },
    { path = "std::eprintln", reason = "eprintln!() has no effect inside a spacetimedb module; use log::warn!() instead" },
    { path = "std::dbg",      reason = "std::dbg!() has no effect inside a spacetimedb module; import spacetime's dbg!() macro instead" },
]
"#;

fn check_for_wasm_bindgen(artifact: &Path) -> anyhow::Result<()> {
    // if this fails for some reason, just let it fail elsewhere
    let Ok(file) = fs::File::open(artifact) else { return Ok(()) };
    let Ok(module) = wasmbin::Module::decode_from(&mut io::BufReader::new(file)) else { return Ok(()) };
    if has_wasm_bindgen(&module) {
        anyhow::bail!(
            "wasm-bindgen detected.\n\
             \n\
             It seems like either you or a crate in your dependency tree is depending on\n\
             wasm-bindgen. wasm-bindgen is only for webassembly modules that target the web\n\
             platform, and will not work in the context of SpacetimeDB.\n\
             \n\
             To find the offending dependency, run `cargo tree -i wasm-bindgen`. Try checking\n\
             its cargo features for 'js' or 'web' or 'wasm-bindgen' to see if there's a way\n\
             to disable it."
        )
    }
    Ok(())
}

const WBINDGEN_PREFIX: &str = "__wbindgen";
fn has_wasm_bindgen(module: &wasmbin::Module) -> bool {
    let check_import = |import: &wasmbin::sections::Import| {
        import.path.module.starts_with(WBINDGEN_PREFIX) || import.path.name.starts_with(WBINDGEN_PREFIX)
    };
    let check_export = |export: &wasmbin::sections::Export| export.name.starts_with(WBINDGEN_PREFIX);

    if let Some(imports) = module.find_std_section::<wasmbin::sections::payload::Import>() {
        if let Ok(imports) = imports.try_contents() {
            if imports.iter().any(check_import) {
                return true;
            }
        }
    }

    if let Some(exports) = module.find_std_section::<wasmbin::sections::payload::Export>() {
        if let Ok(exports) = exports.try_contents() {
            if exports.iter().any(check_export) {
                return true;
            }
        }
    }

    false
}
