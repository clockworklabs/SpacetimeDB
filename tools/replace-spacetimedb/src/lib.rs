#![allow(clippy::disallowed_macros)]
use ignore::{DirEntry, WalkBuilder};
use regex::Regex;
use std::fs;
use std::io;
use std::path::{Path};

#[derive(Clone, Debug)]
pub struct ReplaceOptions {
    pub dry_run: bool,
    pub only_exts: Option<Vec<String>>,
    pub follow_symlinks: bool,
    pub include_hidden: bool,
    pub ignore_globs: Vec<String>,
}

fn is_probably_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0)
}

fn should_process_file(path: &Path, only_exts: &Option<Vec<String>>) -> bool {
    if let Some(exts) = only_exts {
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            return exts.iter().any(|e| e.eq_ignore_ascii_case(ext));
        }
        return false;
    }
    true
}

pub struct ReplaceStats {
    pub files_changed: usize,
    pub occurrences: usize,
}

/// Replace only occurrences inside `} from 'spacetimedb'` or `} from "spacetimedb"`
/// (works for both `import { ... } from ...` and `export { ... } from ...`).
pub fn replace_in_tree(
    root: impl AsRef<Path>,
    replacement: &str,
    options: &ReplaceOptions,
) -> io::Result<ReplaceStats> {
    let root = root.as_ref().to_path_buf();
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Not a directory: {}", root.display()),
        ));
    }

    // Match exactly the two forms you want. No backreferences needed.
    // We intentionally DO NOT include a trailing semicolon so we preserve it (or its absence).
    let re_single = Regex::new(r#"}\s*from\s*'spacetimedb'"#).unwrap();
    let re_double = Regex::new(r#"}\s*from\s*"spacetimedb""#).unwrap();

    let mut builder = WalkBuilder::new(&root);
    builder
        .follow_links(options.follow_symlinks)
        .hidden(!options.include_hidden)
        .git_exclude(true)
        .git_ignore(true)
        .git_global(true);
    builder.add_ignore("node_modules");
    builder.add_ignore("target");
    builder.add_ignore(".git");
    for g in &options.ignore_globs {
        builder.add_ignore(g);
    }

    let mut files_changed = 0usize;
    let mut total_matches = 0usize;

    for result in builder.build() {
        let entry: DirEntry = match result {
            Ok(e) => e,
            Err(err) => {
                eprintln!("walk error: {err}");
                continue;
            }
        };

        let path = entry.path();
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        if !should_process_file(path, &options.only_exts) {
            continue;
        }

        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(err) => {
                eprintln!("read error {}: {err}", path.display());
                continue;
            }
        };
        if !is_probably_text(&bytes) {
            continue;
        }

        let content = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Count before replacing
        let matches = re_single.find_iter(&content).count()
            + re_double.find_iter(&content).count();
        if matches == 0 {
            continue;
        }

        // Do the replacements, preserving quote style
        let updated1 = re_single.replace_all(&content, format!("}} from '{}'", replacement));
        let updated  = re_double.replace_all(&updated1,  format!("}} from \"{}\"", replacement));

        if options.dry_run {
            println!("[dry-run] {} ({} matches)", path.display(), matches);
        } else if let Err(err) = fs::write(path, updated.as_ref()) {
            eprintln!("write error {}: {err}", path.display());
            continue;
        } else {
            println!("✔ {} ({} matches)", path.display(), matches);
        }

        files_changed += 1;
        total_matches += matches;
    }

    Ok(ReplaceStats { files_changed, occurrences: total_matches })
}