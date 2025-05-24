use std::path::{Path, PathBuf};

use crate::util::{self, ModuleLanguage};

use self::csharp::build_csharp;
use self::go::build_go;
use crate::tasks::rust::build_rust;

use duct::cmd;

pub fn build(project_path: &Path, lint_dir: Option<&Path>, build_debug: bool) -> anyhow::Result<PathBuf> {
    let lang = util::detect_module_language(project_path)?;
    let mut wasm_path = match lang {
        ModuleLanguage::Rust => build_rust(project_path, lint_dir, build_debug),
        ModuleLanguage::Csharp => build_csharp(project_path, build_debug),
        ModuleLanguage::Go => build_go(project_path, lint_dir, build_debug),
    }?;
    if !build_debug {
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
    }
    Ok(wasm_path)
}

pub mod csharp;
pub mod go;
pub mod rust;
