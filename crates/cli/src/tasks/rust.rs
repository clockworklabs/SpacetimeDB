use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::Context;
use cargo_metadata::Message;
use duct::cmd;

fn cargo_cmd(subcommand: &str, build_debug: bool, args: &[&str]) -> duct::Expression {
    duct::cmd(
        "cargo",
        [
            subcommand,
            "--config=net.git-fetch-with-cli=true",
            "--target=wasm32-unknown-unknown",
        ]
        .into_iter()
        .chain((!build_debug).then_some("--release"))
        .chain(args.iter().copied()),
    )
}

pub(crate) fn build_rust(project_path: &Path, skip_clippy: bool, build_debug: bool) -> anyhow::Result<PathBuf> {
    // Make sure that we have the wasm target installed (ok to run if its already installed)
    if let Err(err) = cmd!("rustup", "target", "add", "wasm32-unknown-unknown").run() {
        println!(
            "Warning: Failed to install wasm32-unknown-unknown target: {}. Is `rustup` installed?",
            err
        );
    }

    if !skip_clippy {
        let mut err_count: u32 = 0;
        for file in walkdir::WalkDir::new(project_path).into_iter().filter_map(Result::ok) {
            let printable_path = file.path().to_str().ok_or(anyhow::anyhow!("path not utf-8"))?;
            if file.file_type().is_file() && file.path().extension().map_or(false, |ext| ext == "rs") {
                let file = fs::File::open(&file.path())?;
                for (idx, line) in io::BufReader::new(file).lines().enumerate() {
                    let line = line?;
                    let line_number = idx + 1;
                    for disallowed in &["println!", "print!", "eprintln!", "eprint!", "dbg!"] {
                        if line.contains(disallowed) {
                            if err_count == 0 {
                                eprintln!("\nDetected nonfunctional print statements:");
                            }
                            eprintln!("\n{printable_path}:{line_number}: {line}\n");
                            err_count += 1;
                        }
                    }
                }
            }
        }
        if err_count > 0 {
            anyhow::bail!(
                "Found {err_count} disallowed print statement(s).\n\
                These will not be printed from SpacetimeDB modules.\n\
                If you need to print something, use the `log` crate\n\
                and the `log::info!` macro instead."
            );
        }
    }

    let reader = cargo_cmd("build", build_debug, &["--message-format=json-render-diagnostics"])
        .dir(project_path)
        .reader()?;

    let mut artifact = None;
    for message in Message::parse_stream(io::BufReader::new(reader)) {
        match message {
            Ok(Message::CompilerArtifact(art)) => artifact = Some(art),
            Err(error) => return Err(anyhow::anyhow!(error)),
            _ => {}
        }
    }
    let artifact = artifact.context("no artifact found?")?;
    let artifact = artifact.filenames.into_iter().next().context("no wasm?")?;

    check_for_issues(artifact.as_ref())?;

    Ok(artifact.into())
}

fn check_for_issues(artifact: &Path) -> anyhow::Result<()> {
    // if this fails for some reason, just let it fail elsewhere
    let Ok(file) = fs::File::open(artifact) else {
        return Ok(());
    };
    let Ok(module) = wasmbin::Module::decode_from(&mut io::BufReader::new(file)) else {
        return Ok(());
    };
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
    if has_getrandom(&module) {
        anyhow::bail!(
            "getrandom usage detected.\n\
             \n\
             It seems like either you or a crate in your dependency tree is depending on\n\
             the `getrandom` crate for random number generation. getrandom is the default\n\
             randomness source for the `rand` crate, and is used when you call\n\
             `rand::random()` or `rand::thread_rng()`. If this is you, you should instead\n\
             use `spacetimedb::random()` or `spacetimedb::rng()`. If this is a crate in your\n\
             tree, you should try to see if the crate provides a way to pass in a custom\n\
             `Rng` type, and pass it the rng returned from `spacetimedb::rng()`."
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

    module
        .find_std_section::<wasmbin::sections::payload::Import>()
        .and_then(|imports| imports.try_contents().ok())
        .is_some_and(|imports| imports.iter().any(check_import))
        || module
            .find_std_section::<wasmbin::sections::payload::Export>()
            .and_then(|exports| exports.try_contents().ok())
            .is_some_and(|exports| exports.iter().any(check_export))
}

fn has_getrandom(module: &wasmbin::Module) -> bool {
    module
        .find_std_section::<wasmbin::sections::payload::Import>()
        .and_then(|imports| imports.try_contents().ok())
        .is_some_and(|imports| imports.iter().any(|import| import.path.name == "__getrandom_custom"))
}
