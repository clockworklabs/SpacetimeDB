#![allow(clippy::disallowed_macros)]

//! `cargo ci check-style`: measure the Deep crates against `docs/DEEP_DATABASE_STYLE.md`.
//!
//! This is a report. Nothing here fails the build yet. The numbers are a
//! baseline, so that when we do start enforcing, we can record today's values
//! and fail only when one gets worse.
//!
//! ## What gets counted
//!
//! Counting is lexical. Comments and string literals are blanked before any
//! token is matched, so an `assert!` inside a doc comment or an error message
//! is not counted. Only production code is measured: `tests/`, `benches/` and
//! `examples/` directories, `tests.rs` module files, and inline
//! `#[cfg(test)] mod` blocks are skipped, because test code is mostly
//! assertions and would make the assertion density look better than the
//! production paths deserve.
//!
//! Known imprecision: `fns` counts `fn ` tokens, so trait method signatures
//! without a body count as functions, and `.expect(` also matches methods
//! that happen to be named `expect`. The counts are deterministic, which is
//! what a future ratchet needs.

use anyhow::{bail, Context, Result};
use ci_common::ensure_repo_root;
use clap::Parser;
use duct::cmd;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Report how the Deep crates measure up against docs/DEEP_DATABASE_STYLE.md.
///
/// This only reports; it does not enforce anything yet.
#[derive(Parser)]
struct Cli {}

// =============================================================================
// Tiers
// =============================================================================

/// Review tiers, ordered so that a dependency on a lower tier compares less.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Core,
    Deep,
}

impl Tier {
    fn name(self) -> &'static str {
        match self {
            Tier::Core => "core",
            Tier::Deep => "deep",
        }
    }
}

/// The tier of `dir`, a path relative to the repository root: Deep if `dir` or
/// a directory above it has a `.deep` marker, and Core otherwise.
fn tier_of(dir: &Path) -> Tier {
    assert!(
        dir.is_relative(),
        "tier_of takes a path relative to the repository root"
    );
    if dir.ancestors().any(|ancestor| is_marker(&ancestor.join(".deep"))) {
        Tier::Deep
    } else {
        Tier::Core
    }
}

/// Markers are symlinks to the style doc. `symlink_metadata` rather than
/// `exists`, because a link broken by moving a crate to a different depth is
/// still a marker, and must not silently turn the crate into Core.
fn is_marker(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

// =============================================================================
// Workspace
// =============================================================================

struct Dependency {
    name: String,
    /// Dev-dependencies do not ship, so neither the tier rule nor the
    /// dependency count looks at them.
    dev: bool,
    optional: bool,
    /// A path dependency is another crate in this repository. Everything else
    /// comes from a registry.
    path: bool,
}

struct Package {
    name: String,
    /// Relative to the repository root.
    dir: PathBuf,
    tier: Tier,
    /// The library target's root file, if the package has one.
    lib_root: Option<PathBuf>,
    dependencies: Vec<Dependency>,
}

impl Package {
    /// The package name without the `spacetimedb-` prefix every crate shares.
    fn short_name(&self) -> &str {
        self.name.strip_prefix("spacetimedb-").unwrap_or(&self.name)
    }
}

fn workspace_packages() -> Result<Vec<Package>> {
    let root = std::env::current_dir()?;
    let json = cmd!("cargo", "metadata", "--no-deps", "--format-version", "1")
        .read()
        .context("failed to run cargo metadata")?;
    let metadata: Value = serde_json::from_str(&json).context("failed to parse cargo metadata")?;
    let Some(entries) = metadata["packages"].as_array() else {
        bail!("cargo metadata output has no packages array");
    };

    let mut packages = Vec::new();
    for entry in entries {
        let name = str_field(entry, "name")?;
        let manifest = PathBuf::from(str_field(entry, "manifest_path")?);
        let Some(dir) = manifest.parent() else {
            bail!("manifest path {} has no parent directory", manifest.display());
        };
        let dir = dir
            .strip_prefix(&root)
            .with_context(|| format!("package {name} is outside the repository"))?
            .to_path_buf();

        let mut lib_root = None;
        for target in entry["targets"].as_array().into_iter().flatten() {
            let is_lib = target["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|kind| kind.as_str().is_some_and(|kind| kind.ends_with("lib")));
            if is_lib {
                lib_root = Some(PathBuf::from(str_field(target, "src_path")?));
            }
        }

        let mut dependencies = Vec::new();
        for dependency in entry["dependencies"].as_array().into_iter().flatten() {
            dependencies.push(Dependency {
                name: str_field(dependency, "name")?.to_owned(),
                dev: dependency["kind"].as_str() == Some("dev"),
                optional: dependency["optional"].as_bool().unwrap_or(false),
                path: dependency.get("path").is_some_and(|path| !path.is_null()),
            });
        }

        packages.push(Package {
            name: name.to_owned(),
            tier: tier_of(&dir),
            dir,
            lib_root,
            dependencies,
        });
    }
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

fn str_field<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .with_context(|| format!("cargo metadata entry has no string field `{field}`"))
}

/// The production `.rs` files of a package: everything under `src/` that is
/// not test code by its path.
fn production_files(package_dir: &Path) -> Result<Vec<PathBuf>> {
    let src = package_dir.join("src");
    let mut files = Vec::new();
    // An explicit stack rather than recursion, per the deep core's habit.
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") && !is_test_path(path.strip_prefix(&src)?) {
                files.push(path);
            }
        }
    }
    // `read_dir` order is not defined. Sorting makes the report reproducible:
    // same tree, same numbers, same order.
    files.sort();
    assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
    Ok(files)
}

/// Is this file, relative to `src/`, test code rather than crate source?
fn is_test_path(relative: &Path) -> bool {
    let in_test_dir = relative
        .components()
        .any(|part| matches!(part.as_os_str().to_str(), Some("tests" | "benches" | "examples")));
    let is_tests_module = relative.file_stem().is_some_and(|stem| stem == "tests");
    in_test_dir || is_tests_module
}

// =============================================================================
// Metrics
// =============================================================================

/// A style rule measured by counting tokens in production code.
struct Metric {
    header: &'static str,
    /// What the column counts, and which rule in DEEP_DATABASE_STYLE.md it measures.
    legend: &'static str,
    tokens: &'static [&'static str],
}

const FUNCTION_TOKENS: &[&str] = &["fn "];
const ASSERT_TOKENS: &[&str] = &["assert!", "assert_eq!", "assert_ne!"];

const METRICS: &[Metric] = &[
    Metric {
        header: "debug_assert",
        legend: "`debug_assert!` and its `_eq`/`_ne` forms. Principle 8: use `assert!`.",
        tokens: &["debug_assert!", "debug_assert_eq!", "debug_assert_ne!"],
    },
    Metric {
        header: "usize",
        legend: "`usize` and `isize`. Style: use explicit integer sizes.",
        tokens: &["usize", "isize"],
    },
    Metric {
        header: "unwrap",
        legend: "`.unwrap()` and `.expect(`. Style: every `Result` has a planned response.",
        tokens: &[".unwrap()", ".expect("],
    },
    Metric {
        header: "panic",
        legend: "`panic!`, `unreachable!`, `todo!` and `unimplemented!`. Same rule.",
        tokens: &["panic!", "unreachable!", "todo!", "unimplemented!"],
    },
    Metric {
        header: "hash",
        legend: "`HashMap` and `HashSet`, whose iteration order is undefined. Principle 2.",
        tokens: &["HashMap", "HashSet"],
    },
    Metric {
        header: "clock",
        legend: "`Instant::now` and `SystemTime::now`. Principle 2: time arrives as an input.",
        tokens: &["Instant::now", "SystemTime::now"],
    },
    Metric {
        header: "rand",
        legend: "`thread_rng`, `rand::random`, `OsRng` and `getrandom::`. Principle 2: randomness arrives as an input.",
        tokens: &["thread_rng", "rand::random", "OsRng", "getrandom::"],
    },
    Metric {
        header: "spawn",
        legend: "`thread::spawn`, `tokio::spawn`, `task::spawn` and `spawn_blocking`. Principles 2 and 3.",
        tokens: &["thread::spawn", "tokio::spawn", "task::spawn", "spawn_blocking"],
    },
    Metric {
        header: "tokio",
        legend: "`tokio::` paths. Principles 2 and 4.",
        tokens: &["tokio::"],
    },
];

#[derive(Default)]
struct Counts {
    lines: u32,
    functions: u32,
    asserts: u32,
    metrics: [u32; METRICS.len()],
}

impl Counts {
    fn add(&mut self, other: &Counts) {
        self.lines += other.lines;
        self.functions += other.functions;
        self.asserts += other.asserts;
        for (total, count) in self.metrics.iter_mut().zip(other.metrics) {
            *total += count;
        }
    }
}

fn count_file(src: &str) -> Counts {
    let mut counts = Counts::default();
    for line in scan(src).iter().filter(|line| line.has_code && !line.in_test_mod) {
        counts.lines += 1;
        counts.functions += count_tokens(&line.code, FUNCTION_TOKENS);
        counts.asserts += count_tokens(&line.code, ASSERT_TOKENS);
        for (count, metric) in counts.metrics.iter_mut().zip(METRICS) {
            *count += count_tokens(&line.code, metric.tokens);
        }
    }
    counts
}

fn count_tokens(code: &str, tokens: &[&str]) -> u32 {
    tokens.iter().map(|token| count_token(code, token)).sum()
}

/// Count `token` in `code` as a whole token.
///
/// A token that starts with an identifier character must not continue an
/// identifier on its left, so `assert!` does not match inside
/// `debug_assert!`. A token that ends with one must not continue on its right,
/// so `task::spawn` does not match inside `task::spawn_blocking`. Tokens that
/// start or end with punctuation, like `.unwrap()` or `tokio::`, need no check
/// on that side.
fn count_token(code: &str, token: &str) -> u32 {
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let bytes = code.as_bytes();
    let check_left = token.bytes().next().is_some_and(is_ident);
    let check_right = token.bytes().last().is_some_and(is_ident);
    let mut count = 0;
    let mut from = 0;
    while let Some(offset) = code[from..].find(token) {
        let start = from + offset;
        let end = start + token.len();
        let left_ok = !check_left || start == 0 || !is_ident(bytes[start - 1]);
        let right_ok = !check_right || end == bytes.len() || !is_ident(bytes[end]);
        if left_ok && right_ok {
            count += 1;
        }
        from = end;
    }
    count
}

/// Does the crate root declare `#![no_std]`?
fn is_no_std(lib_root: &Path) -> Result<bool> {
    let src = fs::read_to_string(lib_root).with_context(|| format!("failed to read {}", lib_root.display()))?;
    Ok(scan(&src).iter().any(|line| line.code.contains("#![no_std]")))
}

// =============================================================================
// Lexical scan
// =============================================================================

#[derive(Clone, Copy)]
enum ScanContext {
    Code,
    /// Inside a string, having just consumed a backslash.
    ///
    /// A separate state rather than "skip two characters", because the escaped
    /// character may be a newline: a backslash at the end of a line continues
    /// the string. Consuming both at once would swallow the newline and lose a
    /// line from the count.
    StringEscape,
    LineComment,
    /// Rust block comments nest, so this carries a depth.
    BlockComment(u32),
    String,
    RawString(u32),
}

struct Line {
    /// The line with comments and string interiors blanked out.
    code: String,
    has_code: bool,
    in_test_mod: bool,
}

/// Split `src` into lines, blanking comments and string literals.
///
/// State persists across newlines because both block comments and ordinary
/// string literals can span lines.
fn scan(src: &str) -> Vec<Line> {
    let chars: Vec<char> = src.chars().collect();
    let at = |index: usize| chars.get(index).copied();
    let mut lines = Vec::new();
    let mut context = ScanContext::Code;
    let mut code = String::new();
    let mut has_code = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '\n' {
            lines.push(Line {
                code: std::mem::take(&mut code),
                has_code,
                in_test_mod: false,
            });
            has_code = false;
            // A line comment ends at the newline. A string continuation
            // resumes the string on the next line. Every other context
            // carries over.
            match context {
                ScanContext::LineComment => context = ScanContext::Code,
                ScanContext::StringEscape => context = ScanContext::String,
                _ => {}
            }
            i += 1;
            continue;
        }

        match context {
            ScanContext::Code => {
                if c == '/' && at(i + 1) == Some('/') {
                    context = ScanContext::LineComment;
                    code.push_str("  ");
                    i += 2;
                } else if c == '/' && at(i + 1) == Some('*') {
                    context = ScanContext::BlockComment(1);
                    code.push_str("  ");
                    i += 2;
                } else if c == '"' {
                    context = ScanContext::String;
                    has_code = true;
                    code.push(' ');
                    i += 1;
                } else if c == 'r' && matches!(at(i + 1), Some('"' | '#')) {
                    // Raw string: `r"..."`, `r#"..."#`, `r##"..."##`. Anything
                    // else starting with `r` is an identifier.
                    let mut hashes = 0;
                    while at(i + 1 + hashes) == Some('#') {
                        hashes += 1;
                    }
                    if at(i + 1 + hashes) == Some('"') {
                        context = ScanContext::RawString(hashes as u32);
                        code.push(' ');
                        i += 2 + hashes;
                    } else {
                        code.push(c);
                        i += 1;
                    }
                    has_code = true;
                } else if c == '\'' {
                    // Either a char literal or a lifetime. `'a` is a lifetime;
                    // `'a'` and `'\n'` are literals. Guessing wrong only matters
                    // if the skipped text holds a counted token, which a char
                    // literal cannot.
                    has_code = true;
                    code.push(' ');
                    if at(i + 1) == Some('\\') {
                        let mut j = i + 2;
                        while j < chars.len() && chars[j] != '\'' && chars[j] != '\n' {
                            j += 1;
                        }
                        i = if at(j) == Some('\'') { j + 1 } else { j };
                    } else if at(i + 2) == Some('\'') {
                        i += 3;
                    } else {
                        i += 1;
                    }
                } else {
                    if !c.is_whitespace() {
                        has_code = true;
                    }
                    code.push(c);
                    i += 1;
                }
            }
            ScanContext::LineComment => {
                code.push(' ');
                i += 1;
            }
            ScanContext::BlockComment(depth) => {
                if c == '/' && at(i + 1) == Some('*') {
                    context = ScanContext::BlockComment(depth + 1);
                    code.push_str("  ");
                    i += 2;
                } else if c == '*' && at(i + 1) == Some('/') {
                    context = if depth <= 1 {
                        ScanContext::Code
                    } else {
                        ScanContext::BlockComment(depth - 1)
                    };
                    code.push_str("  ");
                    i += 2;
                } else {
                    code.push(' ');
                    i += 1;
                }
            }
            ScanContext::StringEscape => {
                // Exactly one character belongs to the escape. A newline here
                // is handled above, which returns the context to `String`.
                code.push(' ');
                context = ScanContext::String;
                i += 1;
            }
            ScanContext::String => {
                if c == '\\' {
                    context = ScanContext::StringEscape;
                } else if c == '"' {
                    context = ScanContext::Code;
                }
                code.push(' ');
                i += 1;
            }
            ScanContext::RawString(hashes) => {
                let mut seen = 0;
                if c == '"' {
                    while seen < hashes as usize && at(i + 1 + seen) == Some('#') {
                        seen += 1;
                    }
                }
                if c == '"' && seen == hashes as usize {
                    context = ScanContext::Code;
                    i += 1 + seen;
                } else {
                    i += 1;
                }
                code.push(' ');
            }
        }
    }

    // A last line with no trailing newline.
    if !code.is_empty() || has_code {
        lines.push(Line {
            code,
            has_code,
            in_test_mod: false,
        });
    }

    // Every line of the file is accounted for exactly once. A scanner that
    // drops or duplicates lines skews every count downstream while still
    // printing a plausible table.
    let newlines = src.matches('\n').count();
    assert!(
        lines.len() == newlines || lines.len() == newlines + 1,
        "scanned {} lines from a source with {newlines} newlines",
        lines.len(),
    );
    mark_test_modules(&mut lines);
    lines
}

/// Flag the extent of every inline `#[cfg(test)] mod ... { ... }`.
fn mark_test_modules(lines: &mut [Line]) {
    let mut depth: i64 = 0;
    let mut pending_cfg_test = false;
    let mut test_depth: Option<i64> = None;
    // The `#[cfg(test)]` attribute line, so it is attributed to the module too.
    let mut attribute_line: Option<usize> = None;

    for index in 0..lines.len() {
        let is_attribute = lines[index].code.contains("#[cfg(test)]");
        if is_attribute {
            pending_cfg_test = true;
            attribute_line = Some(index);
        }

        let mut opened_here = false;
        // Walk the line so that `mod x {` and braces on the same line are
        // handled in order.
        let has_mod = lines[index].code.contains("mod ");
        for c in lines[index].code.chars() {
            if c == '{' {
                depth += 1;
                if pending_cfg_test && has_mod && test_depth.is_none() {
                    test_depth = Some(depth);
                    pending_cfg_test = false;
                    opened_here = true;
                }
            } else if c == '}' {
                if test_depth == Some(depth) {
                    test_depth = None;
                    lines[index].in_test_mod = true;
                }
                depth -= 1;
            }
        }

        if test_depth.is_some() || opened_here {
            lines[index].in_test_mod = true;
            if let Some(attribute) = attribute_line.filter(|_| opened_here) {
                lines[attribute].in_test_mod = true;
            }
        }
        // A `#[cfg(test)]` on a single item, like one test function, must not
        // arm the next unrelated `mod`.
        if !is_attribute && lines[index].code.contains(';') {
            pending_cfg_test = false;
        }
    }
}

// =============================================================================
// Report
// =============================================================================

struct Row {
    name: String,
    counts: Counts,
    dependencies: String,
    no_std: String,
}

/// Three decimal places, truncated. Two would print almost every crate as
/// `0.00` today, which hides the differences this column is for.
fn format_ratio(numerator: u32, denominator: u32) -> String {
    if denominator == 0 {
        return "-".to_owned();
    }
    let thousandths = u64::from(numerator) * 1000 / u64::from(denominator);
    format!("{}.{:03}", thousandths / 1000, thousandths % 1000)
}

fn print_table(rows: &[Row]) {
    let mut headers = vec!["crate", "lines", "fns", "asserts", "asserts/fn"];
    headers.extend(METRICS.iter().map(|metric| metric.header));
    headers.extend(["deps", "no_std"]);

    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let mut cells = vec![
                row.name.clone(),
                row.counts.lines.to_string(),
                row.counts.functions.to_string(),
                row.counts.asserts.to_string(),
                format_ratio(row.counts.asserts, row.counts.functions),
            ];
            cells.extend(row.counts.metrics.iter().map(u32::to_string));
            cells.extend([row.dependencies.clone(), row.no_std.clone()]);
            cells
        })
        .collect();

    let widths: Vec<usize> = (0..headers.len())
        .map(|column| {
            cells
                .iter()
                .map(|row| row[column].len())
                .chain([headers[column].len()])
                .max()
                .unwrap_or(0)
        })
        .collect();

    let render = |row: &[&str]| {
        let mut out = format!("{:<width$}", row[0], width = widths[0]);
        for (cell, width) in row.iter().zip(&widths).skip(1) {
            out.push_str(&format!("  {cell:>width$}"));
        }
        out
    };
    println!("{}", render(&headers));
    for (index, row) in cells.iter().enumerate() {
        // The last row is the total; rule it off from the crates above.
        if index + 1 == cells.len() {
            println!("{}", "-".repeat(widths.iter().sum::<usize>() + 2 * (widths.len() - 1)));
        }
        println!("{}", render(&row.iter().map(String::as_str).collect::<Vec<_>>()));
    }
}

fn main() -> Result<()> {
    Cli::parse();
    ensure_repo_root()?;

    let packages = workspace_packages()?;
    let deep: Vec<&Package> = packages.iter().filter(|package| package.tier == Tier::Deep).collect();
    if deep.is_empty() {
        bail!("found no crates under a .deep marker");
    }

    let mut rows = Vec::new();
    let mut total = Counts::default();
    let mut all_dependencies = BTreeSet::new();
    let mut no_std_crates = 0;
    for package in &deep {
        let files = production_files(&package.dir)?;
        if files.is_empty() {
            // A row of zeros would read as a clean crate rather than as a bug here.
            bail!("found no production source files in {}", package.dir.display());
        }
        let mut counts = Counts::default();
        for file in &files {
            let src = fs::read_to_string(file).with_context(|| format!("failed to read {}", file.display()))?;
            counts.add(&count_file(&src));
        }

        let dependencies: BTreeSet<&str> = package
            .dependencies
            .iter()
            .filter(|dependency| !dependency.dev && !dependency.path)
            .map(|dependency| dependency.name.as_str())
            .collect();
        all_dependencies.extend(dependencies.iter().copied());

        let no_std = match &package.lib_root {
            Some(lib_root) => is_no_std(lib_root)?,
            None => false,
        };
        no_std_crates += u32::from(no_std);

        total.add(&counts);
        rows.push(Row {
            name: package.short_name().to_owned(),
            counts,
            dependencies: dependencies.len().to_string(),
            no_std: if no_std { "yes" } else { "no" }.to_owned(),
        });
    }
    rows.push(Row {
        name: "total".to_owned(),
        counts: total,
        dependencies: all_dependencies.len().to_string(),
        no_std: format!("{no_std_crates}/{}", deep.len()),
    });

    println!("Deep crates measured against docs/DEEP_DATABASE_STYLE.md. This is a report; nothing is enforced yet.");
    println!("Only production code is counted: tests, benches, examples and inline #[cfg(test)] modules are skipped.");
    println!();
    print_table(&rows);
    println!();
    println!("lines         Non-blank, non-comment lines.");
    println!("fns           `fn ` tokens, including trait method signatures.");
    println!("asserts       `assert!`, `assert_eq!` and `assert_ne!`.");
    println!("asserts/fn    Asserts per function. Principle 8 targets 2.000.");
    for metric in METRICS {
        println!("{:<13} {}", metric.header, metric.legend);
    }
    println!("deps          External dependencies from crates.io. Workspace crates and dev-dependencies are not counted, and the total counts each one once. Principle 1.");
    println!("no_std        Whether the crate root declares `#![no_std]`. Principle 4.");

    println!();
    println!("Dependencies of Deep crates on Core crates (AGENTS.md: a Deep crate must not depend on a Core crate):");
    let mut violations = 0;
    for package in &packages {
        for dependency in package.dependencies.iter().filter(|dependency| !dependency.dev) {
            let Some(target) = packages.iter().find(|candidate| candidate.name == dependency.name) else {
                continue;
            };
            if target.tier < package.tier {
                violations += 1;
                println!(
                    "  {} ({}) -> {} ({}){}",
                    package.short_name(),
                    package.tier.name(),
                    target.short_name(),
                    target.tier.name(),
                    if dependency.optional { ", optional" } else { "" },
                );
            }
        }
    }
    if violations == 0 {
        println!("  none");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production(src: &str, tokens: &[&str]) -> u32 {
        scan(src)
            .iter()
            .filter(|line| !line.in_test_mod)
            .map(|line| count_tokens(&line.code, tokens))
            .sum()
    }

    #[test]
    fn counts_asserts_but_not_debug_asserts() {
        assert_eq!(production("fn f() { assert!(x); }\n", ASSERT_TOKENS), 1);
        assert_eq!(production("assert_eq!(a, b);\nassert_ne!(c, d);\n", ASSERT_TOKENS), 2);
        assert_eq!(
            production("debug_assert!(x);\ndebug_assert_eq!(a, b);\n", ASSERT_TOKENS),
            0
        );
    }

    #[test]
    fn ignores_comments_and_strings() {
        assert_eq!(
            production("// assert!(x);\n/// assert!(x);\n/* assert!(x); */\n", ASSERT_TOKENS),
            0
        );
        assert_eq!(production("assert!(x); // assert!(y);\n", ASSERT_TOKENS), 1);
        assert_eq!(production("let s = \"assert!(x);\";\n", ASSERT_TOKENS), 0);
        assert_eq!(production("let s = r#\"assert!(x);\"#;\n", ASSERT_TOKENS), 0);
        // A comment opener inside a string must not start a comment.
        assert_eq!(production("let s = \"/*\";\nassert!(x);\n", ASSERT_TOKENS), 1);
        // An escaped quote must not end the string early.
        assert_eq!(production("let s = \"\\\" assert!(x);\";\n", ASSERT_TOKENS), 0);
        // Block comments nest.
        assert_eq!(
            production("/* a /* b */ assert!(x); */\nassert!(y);\n", ASSERT_TOKENS),
            1
        );
    }

    #[test]
    fn string_continuation_keeps_its_line() {
        let src = "let s = \"abc\\\n    def\";\nassert!(x);\n";
        assert_eq!(scan(src).len(), 3);
        assert_eq!(production(src, ASSERT_TOKENS), 1);
    }

    #[test]
    fn every_line_is_accounted_for() {
        for src in [
            "a\nb\nc\n",
            "a\nb\nc",
            "",
            "\n\n",
            "/* a\nb */\n",
            "let s = \"a\nb\";\n",
            "'\\\n",
        ] {
            let newlines = src.matches('\n').count();
            let lines = scan(src).len();
            assert!(lines == newlines || lines == newlines + 1, "{src:?}: {lines} lines");
        }
    }

    #[test]
    fn skips_inline_test_modules() {
        let src = "fn prod() {\n    assert!(a);\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert!(b);\n    }\n}\n\nfn after() { assert!(c); }\n";
        assert_eq!(production(src, ASSERT_TOKENS), 2);
        assert_eq!(production(src, FUNCTION_TOKENS), 2);
    }

    #[test]
    fn tokens_respect_identifier_boundaries() {
        assert_eq!(count_token("let fnord = 1; myfn (x)", "fn "), 0);
        assert_eq!(count_token("pub async fn f() {}", "fn "), 1);
        assert_eq!(count_token("x: usize, y: usize_like, z: Vec<usize>", "usize"), 2);
        assert_eq!(count_token("task::spawn(a); task::spawn_blocking(b)", "task::spawn"), 1);
        assert_eq!(count_token("a.unwrap(); b.unwrap_or(c)", ".unwrap()"), 1);
    }

    #[test]
    fn test_paths() {
        assert!(is_test_path(Path::new("tests/partial.rs")));
        assert!(is_test_path(Path::new("tests.rs")));
        assert!(is_test_path(Path::new("repo/tests.rs")));
        assert!(!is_test_path(Path::new("lib.rs")));
        assert!(!is_test_path(Path::new("testsuite.rs")));
    }
}
