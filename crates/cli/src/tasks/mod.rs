use std::path::{Path, PathBuf};

use crate::util::{self, ModuleLanguage};

use crate::tasks::rust::build_rust;

use self::csharp::build_csharp;

pub(crate) fn build(project_path: &Path, skip_clippy: bool, build_debug: bool) -> anyhow::Result<PathBuf> {
    let lang = util::detect_module_language(&project_path);
    match lang {
        ModuleLanguage::Rust => build_rust(project_path, skip_clippy, build_debug),
        ModuleLanguage::Csharp => build_csharp(project_path, build_debug),
    }
}

pub mod csharp;
pub mod rust;
