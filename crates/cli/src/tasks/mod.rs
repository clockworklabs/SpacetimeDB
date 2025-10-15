use std::path::{Path, PathBuf};

use crate::util::{self, ModuleLanguage};

use self::csharp::build_csharp;
use self::javascript::build_javascript;
use self::rust::build_rust;

use duct::cmd;

// TODO: Replace the returned `&'static str` with a copy of `HostType` from core.
pub fn build(
    project_path: &Path,
    lint_dir: Option<&Path>,
    build_debug: bool,
) -> anyhow::Result<(PathBuf, &'static str)> {
    let lang = util::detect_module_language(project_path)?;
    let output_path = match lang {
        ModuleLanguage::Rust => build_rust(project_path, lint_dir, build_debug),
        ModuleLanguage::Csharp => build_csharp(project_path, build_debug),
        ModuleLanguage::Javascript => build_javascript(project_path, build_debug),
    }?;

    if lang == ModuleLanguage::Javascript {
        Ok((output_path, "Js"))
    } else if !build_debug {
        Ok((output_path, "Wasm"))
    } else {
        // for release builds, optimize wasm modules with wasm-opt
        let mut wasm_path = output_path;
        eprintln!("Optimising module with wasm-opt...");
        let wasm_path_opt = wasm_path.with_extension("opt.wasm");
        match cmd!("wasm-opt", "-all", "-g", "-O2", &wasm_path, "-o", &wasm_path_opt).run() {
            Ok(_) => wasm_path = wasm_path_opt,
            // Non-critical error for backward compatibility with users who don't have wasm-opt.
            Err(err) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    eprintln!("Could not find wasm-opt to optimise the module.");
                    eprintln!(
                        "For best performance install wasm-opt from https://github.com/WebAssembly/binaryen/releases."
                    );
                } else {
                    // If wasm-opt exists but failed for some reason, print the error but continue with unoptimised module.
                    // This is to reduce disruption in case we produce a module that wasm-opt can't handle like happened before.
                    eprintln!("Failed to optimise module with wasm-opt: {err}");
                }
                eprintln!("Continuing with unoptimised module.");
            }
        }
        Ok((wasm_path, "Wasm"))
    }
}

pub mod csharp;
pub mod javascript;
pub mod rust;
