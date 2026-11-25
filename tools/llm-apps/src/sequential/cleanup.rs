//! `cleanup`: frees the disk space a finished app holds: dependencies, build output, its
//! git repository and dev-server logs. The source and grading files stay.

use super::{apps_in_variant, subdirs};
use crate::paths;
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(apps: &[PathBuf], all: bool, variant: Option<&str>) -> Result<()> {
    let tool_dir = paths::sequential_dir();
    let apps: Vec<PathBuf> = if all {
        subdirs(&tool_dir).iter().flat_map(|v| apps_in_variant(v)).collect()
    } else if let Some(variant) = variant {
        apps_in_variant(&tool_dir.join(variant))
    } else if apps.is_empty() {
        bail!("name the apps to clean, or pass --all or --variant <name>");
    } else {
        apps.to_vec()
    };
    for app in apps.iter().filter(|a| a.is_dir()) {
        clean(app);
    }
    Ok(())
}

fn clean(app: &Path) {
    println!("Cleaning: {}", app.display());
    let remove_dir = |path: PathBuf, label: &str| {
        if path.is_dir() && fs::remove_dir_all(&path).is_ok() && !label.is_empty() {
            println!("  removed {label}");
        }
    };
    remove_dir(app.join(".git"), ".git");
    remove_dir(app.join("node_modules"), "node_modules");
    for dir in subdirs(app) {
        let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
        remove_dir(dir.join("node_modules"), &format!("{name}/node_modules"));
        remove_dir(dir.join("dist"), "");
        remove_dir(dir.join(".vite"), "");
        remove_logs(&dir);
    }
    remove_logs(app);
    println!("  done");
}

fn remove_logs(dir: &Path) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "log") {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_dependencies_build_output_and_logs_but_keeps_source() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("chat-app-1");
        for f in [
            ".git/HEAD",
            "client/node_modules/x/a.js",
            "client/dist/a.js",
            "client/.vite/a",
            "client/vite.log",
            "dev-server.log",
            "client/src/App.tsx",
            "GRADING_RESULTS.md",
        ] {
            let p = app.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        clean(&app);
        for gone in [
            ".git",
            "client/node_modules",
            "client/dist",
            "client/.vite",
            "client/vite.log",
            "dev-server.log",
        ] {
            assert!(!app.join(gone).exists(), "{gone} should be removed");
        }
        assert!(app.join("client/src/App.tsx").is_file());
        assert!(app.join("GRADING_RESULTS.md").is_file());
    }

    #[test]
    fn finds_apps_in_the_current_layout() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir
            .path()
            .join("sequential-upgrade-20260101/spacetime/results/chat-app-1");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(dir.path().join("sequential-upgrade-20260101/spacetime/telemetry/x")).unwrap();
        assert_eq!(apps_in_variant(dir.path()), vec![app]);
    }
}
