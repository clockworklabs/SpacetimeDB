#![allow(clippy::disallowed_macros)]

//! `cargo ci check-style`: measure the Deep crates against `docs/DEEP_DATABASE_STYLE.md`.
//!
//! This is a report. Nothing here fails the build yet. The numbers are a
//! baseline, so that when we do start enforcing, we can record today's values
//! and fail only when one gets worse.
//!
//! ## What gets counted
//!
//! Each file is parsed with `syn`, and the counts come from its tokens, so an
//! `assert!` inside a comment, a doc comment or an error message is not
//! counted. Only production code is measured: `tests/`, `benches/` and
//! `examples/` directories, `tests.rs` module files, and every item marked
//! `#[cfg(test)]` are skipped, because test code is mostly assertions and
//! would make the assertion density look better than the production paths
//! deserve.
//!
//! Known imprecision: the counts match token sequences, not types, so
//! `.expect(` also matches methods that happen to be named `expect`. Macro
//! bodies are only tokens to `syn`, so a `#[cfg(test)]` inside one is not
//! seen. The counts are deterministic, which is what a future ratchet needs.
//! To forbid any one of these patterns outright, clippy's `unwrap_used`,
//! `disallowed_methods`, `disallowed_types` and `disallowed_macros` lints are
//! the better tool, because they resolve names.

use anyhow::{anyhow, bail, Context, Result};
use ci_common::ensure_repo_root;
use clap::Parser;
use duct::cmd;
use proc_macro2::{Delimiter, LineColumn, Span, TokenStream, TokenTree};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, ImplItem, Item, TraitItem};

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

/// A style rule measured by counting token sequences in production code.
struct Metric {
    header: &'static str,
    /// What the column counts, and which rule in DEEP_DATABASE_STYLE.md it measures.
    legend: &'static str,
    /// Each one is split into words by `pattern_words`.
    tokens: &'static [&'static str],
}

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

/// Count the production code in one source file.
fn count_file(src: &str) -> Result<Counts> {
    // proc-macro2's errors are not `Send`, so they cannot be wrapped as they are.
    let tokens: TokenStream = src.parse().map_err(|error| anyhow!("{error}"))?;
    let file: syn::File = syn::parse2(tokens.clone()).map_err(|error| anyhow!("{error}"))?;
    let mut test_code = TestCode::default();
    test_code.visit_file(&file);
    let words: Vec<Word> = words(tokens)
        .into_iter()
        .filter(|word| !test_code.contains(word.start))
        .collect();

    let mut lines = BTreeSet::new();
    for word in &words {
        lines.extend(word.start.line..=word.end.line);
    }
    let mut counts = Counts {
        lines: u32::try_from(lines.len())?,
        functions: count_functions(&words),
        asserts: count_patterns(&words, ASSERT_TOKENS),
        metrics: [0; METRICS.len()],
    };
    for (count, metric) in counts.metrics.iter_mut().zip(METRICS) {
        *count = count_patterns(&words, metric.tokens);
    }
    Ok(counts)
}

/// `fn` followed by a name: functions and methods, including trait method
/// signatures, but not `fn(..)` pointer types.
fn count_functions(words: &[Word]) -> u32 {
    let mut count = 0;
    for pair in words.windows(2) {
        if pair[0].text == "fn" && pair[1].text.starts_with(is_ident_char) {
            count += 1;
        }
    }
    count
}

fn count_patterns(words: &[Word], patterns: &[&str]) -> u32 {
    patterns.iter().map(|pattern| count_pattern(words, pattern)).sum()
}

/// Count the places where the words of `pattern` appear in `words`, adjacent
/// and in order. Words are whole tokens, so `assert!` does not match inside
/// `debug_assert!`, and `task::spawn` does not match `task::spawn_blocking`.
fn count_pattern(words: &[Word], pattern: &str) -> u32 {
    let pattern = pattern_words(pattern);
    assert!(!pattern.is_empty(), "a pattern must have at least one word");
    let mut count = 0;
    for window in words.windows(pattern.len()) {
        if window.iter().zip(&pattern).all(|(word, part)| word.text == *part) {
            count += 1;
        }
    }
    count
}

/// Split a pattern like `Instant::now` or `.unwrap()` into the words it
/// matches: identifiers whole, and punctuation one character at a time, which
/// is how proc-macro2 hands both to us.
fn pattern_words(pattern: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut ident_start = None;
    for (index, c) in pattern.char_indices() {
        if is_ident_char(c) {
            ident_start.get_or_insert(index);
            continue;
        }
        if let Some(start) = ident_start.take() {
            words.push(&pattern[start..index]);
        }
        if !c.is_whitespace() {
            words.push(&pattern[index..index + c.len_utf8()]);
        }
    }
    if let Some(start) = ident_start {
        words.push(&pattern[start..]);
    }
    words
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Does the crate root declare `#![no_std]`?
fn is_no_std(lib_root: &Path) -> Result<bool> {
    let src = fs::read_to_string(lib_root).with_context(|| format!("failed to read {}", lib_root.display()))?;
    let file = syn::parse_file(&src).map_err(|error| anyhow!("failed to parse {}: {error}", lib_root.display()))?;
    // A file's `attrs` are its inner attributes, the `#![...]` ones.
    Ok(file.attrs.iter().any(|attr| attr.path().is_ident("no_std")))
}

// =============================================================================
// Parsing
// =============================================================================

/// One token of source as text: an identifier, a punctuation character, a
/// literal, or one side of a delimiter.
struct Word {
    text: String,
    start: LineColumn,
    end: LineColumn,
}

impl Word {
    fn new(text: String, span: Span) -> Word {
        Word {
            text,
            start: span.start(),
            end: span.end(),
        }
    }
}

/// A token stream being flattened, and how far into it we are.
struct Frame {
    trees: Vec<TokenTree>,
    next: usize,
    /// The closing delimiter, emitted once `trees` is done.
    close: Option<Word>,
}

/// Flatten `tokens` into words. Doc comments reach us as `#[doc = "..."]`
/// attributes and are dropped, so they count as neither code nor lines.
fn words(tokens: TokenStream) -> Vec<Word> {
    let mut words = Vec::new();
    // An explicit stack rather than recursion, per the deep core's habit.
    let mut stack = vec![Frame {
        trees: tokens.into_iter().collect(),
        next: 0,
        close: None,
    }];
    while let Some(frame) = stack.last_mut() {
        if frame.next == frame.trees.len() {
            words.extend(frame.close.take());
            stack.pop();
            continue;
        }
        let doc = doc_attribute_len(&frame.trees[frame.next..]);
        if doc > 0 {
            frame.next += doc;
            continue;
        }
        let tree = frame.trees[frame.next].clone();
        frame.next += 1;
        match tree {
            TokenTree::Group(group) => {
                let delimiters = match group.delimiter() {
                    Delimiter::Parenthesis => Some(("(", ")")),
                    Delimiter::Brace => Some(("{", "}")),
                    Delimiter::Bracket => Some(("[", "]")),
                    // Invisible delimiters, which only macro expansion makes.
                    Delimiter::None => None,
                };
                let close = delimiters.map(|(open, close)| {
                    words.push(Word::new(open.to_owned(), group.span_open()));
                    Word::new(close.to_owned(), group.span_close())
                });
                stack.push(Frame {
                    trees: group.stream().into_iter().collect(),
                    next: 0,
                    close,
                });
            }
            TokenTree::Ident(ident) => words.push(Word::new(ident.to_string(), ident.span())),
            TokenTree::Punct(punct) => words.push(Word::new(punct.as_char().to_string(), punct.span())),
            TokenTree::Literal(literal) => words.push(Word::new(literal.to_string(), literal.span())),
        }
    }
    words
}

/// How many tokens a doc attribute at the start of `trees` takes up: `#` and
/// `[doc ...]`, with a `!` between them for an inner one. Zero if `trees` does
/// not start with one.
fn doc_attribute_len(trees: &[TokenTree]) -> usize {
    let is_punct =
        |tree: Option<&TokenTree>, c: char| matches!(tree, Some(TokenTree::Punct(punct)) if punct.as_char() == c);
    if !is_punct(trees.first(), '#') {
        return 0;
    }
    let bang = usize::from(is_punct(trees.get(1), '!'));
    match trees.get(1 + bang) {
        Some(TokenTree::Group(group))
            if group.delimiter() == Delimiter::Bracket
                && matches!(group.stream().into_iter().next(), Some(TokenTree::Ident(ident)) if ident == "doc") =>
        {
            2 + bang
        }
        _ => 0,
    }
}

/// The extent of every item marked `#[cfg(test)]`, collected by walking the
/// syntax tree.
#[derive(Default)]
struct TestCode {
    extents: Vec<(LineColumn, LineColumn)>,
}

impl TestCode {
    fn contains(&self, at: LineColumn) -> bool {
        self.extents.iter().any(|&(start, end)| start <= at && at < end)
    }

    /// Record `node` if `attrs` mark it as test code, and say whether they did.
    fn record(&mut self, attrs: &[Attribute], node: &impl Spanned) -> bool {
        let is_test = attrs.iter().any(is_cfg_test);
        if is_test {
            let span = node.span();
            self.extents.push((span.start(), span.end()));
        }
        is_test
    }
}

// Each method stops at a test node, because its extent already covers
// everything inside it.
impl<'ast> Visit<'ast> for TestCode {
    fn visit_file(&mut self, file: &'ast syn::File) {
        // `#![cfg(test)]` at the top makes the whole file test code.
        if !self.record(&file.attrs, file) {
            visit::visit_file(self, file);
        }
    }

    fn visit_item(&mut self, item: &'ast Item) {
        if !self.record(item_attrs(item), item) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        if !self.record(impl_item_attrs(item), item) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        if !self.record(trait_item_attrs(item), item) {
            visit::visit_trait_item(self, item);
        }
    }
}

/// Exactly `#[cfg(test)]`. A compound condition like
/// `#[cfg(any(test, feature = "x"))]` can be compiled outside tests, so it
/// does not mark test code.
fn is_cfg_test(attr: &Attribute) -> bool {
    attr.path().is_ident("cfg") && attr.parse_args::<syn::Ident>().is_ok_and(|ident| ident == "test")
}

// syn has no common accessor for a node's attributes, so these three match on
// every kind of item that has them.

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn impl_item_attrs(item: &ImplItem) -> &[Attribute] {
    match item {
        ImplItem::Const(item) => &item.attrs,
        ImplItem::Fn(item) => &item.attrs,
        ImplItem::Type(item) => &item.attrs,
        ImplItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

fn trait_item_attrs(item: &TraitItem) -> &[Attribute] {
    match item {
        TraitItem::Const(item) => &item.attrs,
        TraitItem::Fn(item) => &item.attrs,
        TraitItem::Type(item) => &item.attrs,
        TraitItem::Macro(item) => &item.attrs,
        _ => &[],
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
            counts.add(&count_file(&src).with_context(|| format!("failed to parse {}", file.display()))?);
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
    println!("Only production code is counted: tests, benches, examples and items marked #[cfg(test)] are skipped.");
    println!();
    print_table(&rows);
    println!();
    println!("lines         Lines with code on them. Blank lines, comments and doc comments do not count.");
    println!(
        "fns           `fn` followed by a name, which includes trait method signatures but not `fn(..)` pointer types."
    );
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

    fn counts(src: &str) -> Counts {
        count_file(src).expect("test source should parse")
    }

    fn count(src: &str, pattern: &str) -> u32 {
        let tokens: TokenStream = src.parse().expect("test source should lex");
        count_pattern(&words(tokens), pattern)
    }

    #[test]
    fn counts_asserts_but_not_debug_asserts() {
        assert_eq!(counts("fn f() { assert!(x); }").asserts, 1);
        assert_eq!(counts("fn f() { assert_eq!(a, b); assert_ne!(c, d); }").asserts, 2);
        assert_eq!(
            counts("fn f() { debug_assert!(x); debug_assert_eq!(a, b); }").asserts,
            0
        );
    }

    #[test]
    fn ignores_comments_and_strings() {
        assert_eq!(
            counts("// assert!(x);\n/// assert!(x);\n/* assert!(x); */\nfn f() {}\n").asserts,
            0
        );
        assert_eq!(counts("fn f() { assert!(x); } // assert!(y);\n").asserts, 1);
        assert_eq!(counts("fn f() { let s = \"assert!(x);\"; }").asserts, 0);
        assert_eq!(counts("fn f() { let s = r#\"assert!(x);\"#; }").asserts, 0);
        // Block comments nest.
        assert_eq!(
            counts("/* a /* b */ assert!(x); */\nfn f() { assert!(y); }\n").asserts,
            1
        );
    }

    #[test]
    fn counts_lines_with_code() {
        // Only the function's three lines count.
        let src = "//! Crate docs.\n\n/// Docs.\n// A comment.\nfn f() {\n    g();\n}\n";
        assert_eq!(counts(src).lines, 3);
        // A string that spans lines counts every line it covers.
        assert_eq!(counts("const S: &str = \"a\nb\";\n").lines, 2);
    }

    #[test]
    fn skips_test_items() {
        let src = "\
fn prod() {
    assert!(a);
}

#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        assert!(b);
    }
}

struct S;

impl S {
    #[cfg(test)]
    fn check(&self) {
        assert!(c);
    }

    fn after(&self) { assert!(d); }
}

#[cfg(test)]
impl S {
    fn helper(&self) { assert!(e); }
}

#[cfg(any(test, feature = \"x\"))]
fn shared() { assert!(f); }
";
        let counts = counts(src);
        // `a`, `d` and `f`, in `prod`, `after` and `shared`.
        assert_eq!(counts.asserts, 3);
        assert_eq!(counts.functions, 3);
    }

    #[test]
    fn skips_test_files() {
        assert_eq!(counts("#![cfg(test)]\nfn f() { assert!(x); }\n").asserts, 0);
    }

    #[test]
    fn counts_functions_but_not_fn_pointers() {
        assert_eq!(
            counts("fn f(callback: fn(u32) -> u32) {}\ntrait T { fn g(&self); }\n").functions,
            2
        );
    }

    #[test]
    fn patterns_match_whole_tokens() {
        assert_eq!(count("x: usize, y: usize_like, z: Vec<usize>", "usize"), 2);
        assert_eq!(count("task::spawn(a); task::spawn_blocking(b)", "task::spawn"), 1);
        assert_eq!(count("a.unwrap(); b.unwrap_or(c); d . unwrap ( )", ".unwrap()"), 2);
        assert_eq!(count("Instant :: now()", "Instant::now"), 1);
        assert_eq!(count("debug_assert!(x)", "assert!"), 0);
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
