//! Checks that every `https://spacetimedb.com/docs/...` URL in the repository
//! resolves to a page of the freshly built docs site.
//!
//! Docusaurus already rejects broken relative links inside `docs/`, but it
//! treats absolute URLs as external and never sees files outside `docs/`, such
//! as READMEs, templates and doc comments.

use anyhow::{Context, Result};
use duct::cmd;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

/// Redirect stubs pointing at other redirect stubs are followed this many times.
const MAX_REDIRECTS: usize = 5;

struct BrokenLink {
    file: String,
    line: usize,
    url: String,
    reason: String,
}

/// Returns a description of every broken docs link in the files tracked by git.
pub fn check(repo_root: &Path, build_dir: &Path) -> Result<Vec<String>> {
    let url_re = Regex::new(r#"https?://(?:www\.)?spacetimedb\.com/docs(?:[/?#][^\s()\[\]<>"'`,*|{}\\]*)?"#)?;
    let mut broken = Vec::new();
    let mut checked = 0;

    for file in files_mentioning_docs(repo_root)? {
        let Ok(contents) = fs::read_to_string(repo_root.join(&file)) else {
            continue;
        };
        for (idx, line) in contents.lines().enumerate() {
            for m in url_re.find_iter(line) {
                let url = m.as_str().trim_end_matches(['.', ':', ';', '!', '?']);
                checked += 1;
                if let Err(reason) = resolve(build_dir, url) {
                    broken.push(BrokenLink {
                        file: file.clone(),
                        line: idx + 1,
                        url: url.to_owned(),
                        reason,
                    });
                }
            }
        }
    }

    println!("Checked {checked} links to the docs site, {} broken.", broken.len());
    Ok(broken
        .into_iter()
        .map(|b| format!("{}:{}: {} ({})", b.file, b.line, b.url, b.reason))
        .collect())
}

fn files_mentioning_docs(repo_root: &Path) -> Result<Vec<String>> {
    // `git grep` exits with status 1 when nothing matches, so don't treat that as a failure.
    let output = cmd!("git", "grep", "-lIF", "spacetimedb.com/docs")
        .dir(repo_root)
        .unchecked()
        .read()
        .context("failed to run git grep")?;
    Ok(output.lines().map(str::to_owned).collect())
}

/// Maps a docs URL to the built file that serves it, and checks its anchor, if any.
fn resolve(build_dir: &Path, url: &str) -> Result<(), String> {
    let path = &url[url.find("/docs").unwrap() + "/docs".len()..];
    let (path, fragment) = match path.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (path, None),
    };
    let path = path.split_once('?').map_or(path, |(path, _)| path);

    let mut path = path.to_owned();
    for _ in 0..MAX_REDIRECTS {
        let file = page_file(build_dir, &path).ok_or("no such page")?;
        let Ok(html) = fs::read_to_string(&file) else {
            // Not text, e.g. an image. It exists, which is all we can check.
            return Ok(());
        };
        if let Some(target) = redirect_target(&html) {
            path = target;
            continue;
        }
        return match fragment {
            Some(fragment) if !fragment.is_empty() && !html.contains(&format!(r#"id="{fragment}""#)) => {
                Err(format!("no anchor #{fragment}"))
            }
            _ => Ok(()),
        };
    }
    Err("too many redirects".into())
}

fn page_file(build_dir: &Path, path: &str) -> Option<PathBuf> {
    let path = path.trim_matches('/');
    let base = build_dir.join(path);
    [base.join("index.html"), base.with_extension("html"), base]
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// Returns the path, relative to `/docs`, that a `@docusaurus/plugin-client-redirects` stub points to.
fn redirect_target(html: &str) -> Option<String> {
    if !html.contains(r#"http-equiv="refresh""#) {
        return None;
    }
    let start = html.find("url=/docs")? + "url=/docs".len();
    let end = start + html[start..].find('"')?;
    Some(html[start..end].to_owned())
}
