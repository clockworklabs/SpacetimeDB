#![allow(clippy::disallowed_macros)]

use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Generates the SDK name conformance tests from tools/sdk-names/manifest.toml
///
/// Writes one test per language that names every current and deprecated name in the manifest,
/// so that the build fails if a name disappears, checks the Unreal names textually, and checks
/// that every name follows the naming scheme or records a deviation. Fails if a generated test
/// was out of date, after rewriting it. See tools/sdk-names/README.md.
#[derive(Parser)]
struct Cli {
    /// Do not rewrite out-of-date tests; only fail.
    #[arg(long)]
    check: bool,
}

const MANIFEST: &str = "tools/sdk-names/manifest.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    instances: BTreeMap<String, BTreeMap<String, String>>,
    role: Vec<Role>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Role {
    id: String,
    name: Option<String>,
    description: String,
    rust_module: Cell,
    rust_client: Cell,
    ts_module: Cell,
    ts_client: Cell,
    csharp_module: Cell,
    csharp_client: Cell,
    unreal: Cell,
    cpp_module: Cell,
    #[serde(default)]
    deviations: BTreeMap<String, String>,
    note: Option<String>,
}

/// One language's name for a role: the string `"none"`, or an item.
#[derive(Deserialize)]
#[serde(untagged)]
enum Cell {
    None(String),
    Item(Item),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    /// The name, from the SDK or module library.
    sdk: Option<String>,
    /// The name, or a pattern such as `{Table}TableHandle`, from generated code.
    generated: Option<String>,
    /// Where the name lives, if not in the column's default location.
    path: Option<String>,
    /// Former names that must still resolve.
    #[serde(default)]
    deprecated: Vec<String>,
    /// A Rust `cfg` predicate that the name requires.
    cfg: Option<String>,
    /// `"class"` for a TypeScript class, whose names must also resolve as values.
    kind: Option<String>,
    /// Why no test checks this name.
    unchecked: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    RustModule,
    RustClient,
    TsModule,
    TsClient,
    CsharpModule,
    CsharpClient,
    Unreal,
    CppModule,
}

impl Column {
    const ALL: [Column; 8] = [
        Column::RustModule,
        Column::RustClient,
        Column::TsModule,
        Column::TsClient,
        Column::CsharpModule,
        Column::CsharpClient,
        Column::Unreal,
        Column::CppModule,
    ];

    fn key(self) -> &'static str {
        match self {
            Column::RustModule => "rust_module",
            Column::RustClient => "rust_client",
            Column::TsModule => "ts_module",
            Column::TsClient => "ts_client",
            Column::CsharpModule => "csharp_module",
            Column::CsharpClient => "csharp_client",
            Column::Unreal => "unreal",
            Column::CppModule => "cpp_module",
        }
    }
}

impl Role {
    fn cell(&self, column: Column) -> &Cell {
        match column {
            Column::RustModule => &self.rust_module,
            Column::RustClient => &self.rust_client,
            Column::TsModule => &self.ts_module,
            Column::TsClient => &self.ts_client,
            Column::CsharpModule => &self.csharp_module,
            Column::CsharpClient => &self.csharp_client,
            Column::Unreal => &self.unreal,
            Column::CppModule => &self.cpp_module,
        }
    }
}

/// One name that a conformance test checks.
struct Name {
    role: String,
    column: Column,
    generated: bool,
    /// The item's location, if not the column's default. Placeholders are filled in.
    path: Option<String>,
    /// The item's name. Placeholders are filled in, except in the Unreal column.
    name: String,
    deprecated: bool,
    /// For a deprecated name, the current name of the same role, if there is one.
    replacement: Option<String>,
    cfg: Option<String>,
    /// Whether the name must resolve as a value, as a TypeScript class must.
    value: bool,
}

fn validate(manifest: &Manifest) -> Result<()> {
    let columns: BTreeSet<_> = Column::ALL.iter().map(|c| c.key()).collect();
    for key in manifest.instances.keys() {
        ensure!(columns.contains(key.as_str()), "unknown column `{key}` in [instances]");
    }
    let mut ids = BTreeSet::new();
    for role in &manifest.role {
        let id = &role.id;
        ensure!(ids.insert(id.as_str()), "role `{id}` appears twice");
        ensure!(
            !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c == '.'),
            "role id `{id}` must be lowercase words separated by `.` or `_`"
        );
        for text in [Some(&role.description), role.name.as_ref(), role.note.as_ref()]
            .into_iter()
            .flatten()
        {
            ensure!(
                !text.is_empty() && !text.contains('\n'),
                "role `{id}` has an empty or multi-line field"
            );
        }
        for (key, reason) in &role.deviations {
            ensure!(
                columns.contains(key.as_str()),
                "role `{id}` has a deviation for unknown column `{key}`"
            );
            ensure!(
                !reason.is_empty() && !reason.contains('\n'),
                "role `{id}` needs a one-line reason for `{key}`"
            );
        }
        for column in Column::ALL {
            let key = column.key();
            match role.cell(column) {
                Cell::None(none) => ensure!(none == "none", "role `{id}`: `{key}` must be \"none\" or an item"),
                Cell::Item(item) => {
                    ensure!(
                        !(item.sdk.is_some() && item.generated.is_some()),
                        "role `{id}`: `{key}` cannot be both `sdk` and `generated`"
                    );
                    ensure!(
                        item.sdk.is_some() || item.generated.is_some() || !item.deprecated.is_empty(),
                        "role `{id}`: `{key}` needs a name, deprecated names, or \"none\""
                    );
                    ensure!(
                        item.cfg.is_none() || matches!(column, Column::RustModule | Column::RustClient),
                        "role `{id}`: `cfg` applies only to Rust columns"
                    );
                    ensure!(
                        item.kind.is_none()
                            || (item.kind.as_deref() == Some("class")
                                && matches!(column, Column::TsModule | Column::TsClient)),
                        "role `{id}`: `kind` can only be \"class\", in a TypeScript column"
                    );
                }
            }
        }
    }
    Ok(())
}

/// Replaces each `{Key}` in `text` with the column's instance of `Key`.
fn fill(text: &str, instances: Option<&BTreeMap<String, String>>, column: Column, role: &str) -> Result<String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let end = rest[start..]
            .find('}')
            .with_context(|| format!("role `{role}`: unclosed placeholder in `{text}`"))?;
        let key = &rest[start + 1..start + end];
        let value = instances.and_then(|i| i.get(key)).with_context(|| {
            format!(
                "role `{role}`: no instance of `{{{key}}}` for `{}`; add one to [instances.{}]",
                column.key(),
                column.key()
            )
        })?;
        out.push_str(&rest[..start]);
        out.push_str(value);
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn collect_names(manifest: &Manifest) -> Result<Vec<Name>> {
    let mut names = Vec::new();
    for role in &manifest.role {
        for column in Column::ALL {
            let Cell::Item(item) = role.cell(column) else {
                continue;
            };
            if item.unchecked.is_some() {
                continue;
            }
            let instances = manifest.instances.get(column.key());
            // Unreal names are matched as patterns, so they keep their placeholders.
            let fill_in = |text: &str| -> Result<String> {
                if column == Column::Unreal {
                    Ok(text.to_owned())
                } else {
                    fill(text, instances, column, &role.id)
                }
            };
            let path = item.path.as_deref().map(fill_in).transpose()?;
            let current = item
                .sdk
                .as_deref()
                .or(item.generated.as_deref())
                .map(fill_in)
                .transpose()?;
            let mut push = |name: String, deprecated: bool| {
                names.push(Name {
                    role: role.id.clone(),
                    column,
                    generated: item.generated.is_some(),
                    path: path.clone(),
                    name,
                    deprecated,
                    replacement: if deprecated { current.clone() } else { None },
                    cfg: item.cfg.clone(),
                    value: item.kind.is_some(),
                });
            };
            if let Some(current) = &current {
                push(current.clone(), false);
            }
            for old in &item.deprecated {
                push(fill_in(old)?, true);
            }
        }
    }
    Ok(names)
}

fn names_in(names: &[Name], column: Column, generated: bool) -> impl Iterator<Item = &Name> {
    names
        .iter()
        .filter(move |n| n.column == column && n.generated == generated)
}

struct Output {
    path: &'static str,
    contents: String,
}

const RUST_HEADER: &str = "\
//! Checks that every name in the SDK name manifest still resolves. The test passes if it compiles.
//!
//! Generated by `cargo ci sdk-names` from `tools/sdk-names/manifest.toml`. Do not edit.
";

/// A Rust module of `use path::Name as _;` items, one per name. An SDK name's path is absolute,
/// and a generated name's path is relative to `bindings`.
fn rust_uses<'a>(names: impl Iterator<Item = &'a Name>, prelude: &str, sdk: &str, bindings: &str) -> String {
    let mut out = format!("{RUST_HEADER}\n#[rustfmt::skip]\n#[allow(unused_imports)]\nmod names {{\n{prelude}");
    for name in names {
        let path = match (&name.path, name.generated) {
            (Some(path), true) => format!("{bindings}::{path}"),
            (Some(path), false) => path.clone(),
            (None, true) => bindings.to_owned(),
            (None, false) => sdk.to_owned(),
        };
        if let Some(cfg) = &name.cfg {
            writeln!(out, "    #[cfg({cfg})]").unwrap();
        }
        if name.deprecated {
            writeln!(out, "    #[allow(deprecated)]").unwrap();
        }
        writeln!(out, "    use {path}::{} as _; // {}", name.name, name.role).unwrap();
    }
    out.push_str("}\n");
    out
}

const TS_HEADER: &str = "\
// Checks that every name in the SDK name manifest still resolves. The test
// passes if it type-checks.
//
// Generated by `cargo ci sdk-names` from tools/sdk-names/manifest.toml. Do not
// edit.

/* eslint-disable @typescript-eslint/no-unused-vars */
";

/// Writes `{open}a, b{close}` on one line if it fits in 80 columns, and otherwise one item per
/// line, as Prettier formats imports and arrays.
fn ts_list(out: &mut String, open: &str, items: &[&str], close: &str) {
    let line = format!("{open}{}{close}", items.join(", "));
    if line.len() <= 80 {
        writeln!(out, "{line}").unwrap();
    } else {
        writeln!(out, "{}", open.trim_end()).unwrap();
        for item in items {
            writeln!(out, "  {item},").unwrap();
        }
        writeln!(out, "{}", close.trim_start()).unwrap();
    }
}

/// A TypeScript file that imports every name, grouped by module specifier: types with
/// `import type`, and classes with a value import and a use, so that a class's value must
/// resolve too.
fn ts_imports<'a>(names: impl Iterator<Item = &'a Name>, specifier: impl Fn(&Name) -> String) -> Result<String> {
    // Each specifier's names, in order, with whether each must resolve as a value.
    let mut groups: Vec<(String, Vec<(&str, bool)>)> = Vec::new();
    let mut seen = BTreeMap::new();
    for name in names {
        let from = specifier(name);
        if let Some(previous) = seen.insert(name.name.as_str(), from.clone()) {
            ensure!(
                previous == from,
                "TypeScript name `{}` comes from both `{previous}` and `{from}`",
                name.name
            );
        }
        let group = match groups.iter().position(|(f, _)| *f == from) {
            Some(i) => &mut groups[i].1,
            None => {
                groups.push((from, Vec::new()));
                &mut groups.last_mut().unwrap().1
            }
        };
        match group.iter_mut().find(|(n, _)| *n == name.name) {
            Some((_, value)) => *value |= name.value,
            None => group.push((&name.name, name.value)),
        }
    }
    let mut out = TS_HEADER.to_owned();
    let mut values = Vec::new();
    for (from, group) in &groups {
        let types: Vec<_> = group.iter().filter(|(_, v)| !v).map(|(n, _)| *n).collect();
        let classes: Vec<_> = group.iter().filter(|(_, v)| *v).map(|(n, _)| *n).collect();
        if !types.is_empty() {
            ts_list(&mut out, "import type { ", &types, &format!(" }} from '{from}';"));
        }
        if !classes.is_empty() {
            ts_list(&mut out, "import { ", &classes, &format!(" }} from '{from}';"));
        }
        values.extend(classes);
    }
    if !values.is_empty() {
        out.push_str("\n// Each class must also resolve as a value.\n");
        ts_list(&mut out, "const _values = [", &values, "];");
    }
    Ok(out)
}

/// The module specifier, relative to `src/lib`, for a name in the TypeScript package.
fn ts_sdk_specifier(name: &Name, default: &str) -> String {
    let path = name.path.as_deref().unwrap_or(default);
    if let Some(entry) = path.strip_prefix("spacetimedb") {
        format!("..{entry}")
    } else {
        let file = path.trim_start_matches("src/").trim_end_matches(".ts");
        match file.strip_prefix("lib/") {
            Some(file) => format!("./{file}"),
            None => format!("../{file}"),
        }
    }
}

const CSHARP_HEADER: &str = "\
// <auto-generated />
// Checks that every name in the SDK name manifest still resolves. The check passes if this
// file compiles. Generated by `cargo ci sdk-names` from tools/sdk-names/manifest.toml. Do not
// edit.
";

/// `Name<A, B>` becomes `Name<,>`, the form `typeof` takes for an open generic type.
fn csharp_open_generic(name: &str) -> String {
    match name.find('<') {
        Some(start) => format!("{}<{}>", &name[..start], ",".repeat(name[start..].matches(',').count())),
        None => name.to_owned(),
    }
}

/// A C# class that takes `typeof` of every name.
fn csharp_typeofs<'a>(
    names: impl Iterator<Item = &'a Name>,
    preamble: &str,
    sdk_namespace: &str,
    generated_namespace: &str,
) -> String {
    let mut out = format!(
        "{CSHARP_HEADER}{preamble}\ninternal static class SdkNames\n{{\n    internal static void Check()\n    {{\n"
    );
    for name in names {
        let default = if name.generated {
            generated_namespace
        } else {
            sdk_namespace
        };
        let namespace = name.path.as_deref().unwrap_or(default);
        let ty = csharp_open_generic(&name.name);
        writeln!(out, "        _ = typeof(global::{namespace}.{ty}); // {}", name.role).unwrap();
    }
    out.push_str("    }\n}\n");
    out
}

/// `name` qualified by the namespace of `item`: `SpacetimeDB` by default for an SDK name, and the
/// global namespace for a name that a macro generates.
fn cpp_qualified(item: &Name, name: &str) -> String {
    let default = if item.generated { "" } else { "SpacetimeDB" };
    let bare = name.split('<').next().unwrap();
    format!("{}::{bare}", item.path.as_deref().unwrap_or(default))
}

/// The number of template parameters in a name such as `Table<T>`.
fn cpp_params(name: &str) -> usize {
    name.find('<').map_or(0, |start| name[start..].matches(',').count() + 1)
}

/// The type that the C++ case passes for a template's parameter `index`.
fn cpp_arg(index: usize) -> String {
    format!("SdkNamesArg{index}")
}

/// `name` as a type: a template such as `Table<T>` instantiated with a distinct type for each
/// parameter, so that an alias template that reorders or repeats its parameters names a
/// different type than its replacement.
fn cpp_type(item: &Name, name: &str) -> String {
    match cpp_params(name) {
        0 => cpp_qualified(item, name),
        params => {
            let args: Vec<_> = (0..params).map(cpp_arg).collect();
            format!("{}<{}>", cpp_qualified(item, name), args.join(", "))
        }
    }
}

/// A C++ compile case with a using-declaration for every name, and a `static_assert` that each
/// deprecated alias names the same type as its replacement. Templates are compared instantiated
/// with a distinct incomplete type for each parameter. Naming a specialization does not
/// instantiate it, so the types need no definitions.
fn cpp_case<'a>(names: impl Iterator<Item = &'a Name> + Clone) -> Result<String> {
    let mut out = String::from(
        "\
// Checks that every name in the SDK name manifest still resolves. The case passes if it compiles.
//
// Generated by `cargo ci sdk-names` from tools/sdk-names/manifest.toml. Do not edit.

#include \"spacetimedb.h\"

#include <cstdint>
#include <type_traits>

using namespace SpacetimeDB;

struct SdkNamesRow {
    uint32_t id;
};

SPACETIMEDB_STRUCT(SdkNamesRow, id)
SPACETIMEDB_TABLE(SdkNamesRow, sdk_names_row, Public)

namespace sdk_names {
",
    );
    for name in names.clone().filter(|n| !n.deprecated) {
        writeln!(out, "using {}; // {}", cpp_qualified(name, &name.name), name.role).unwrap();
    }
    out.push_str(
        "\
} // namespace sdk_names

// Each deprecated name must still name the type that replaced it.
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored \"-Wdeprecated-declarations\"
namespace sdk_names_deprecated {
",
    );
    let deprecated: Vec<_> = names.filter(|n| n.deprecated).collect();
    for name in &deprecated {
        writeln!(out, "using {}; // {}", cpp_qualified(name, &name.name), name.role).unwrap();
    }
    out.push_str("} // namespace sdk_names_deprecated\n");
    let params = deprecated
        .iter()
        .filter(|n| n.replacement.is_some())
        .map(|n| cpp_params(&n.name))
        .max()
        .unwrap_or(0);
    if params > 0 {
        out.push_str("// A distinct type for each template parameter, so that a reordered or repeated one fails.\n");
    }
    for index in 0..params {
        writeln!(out, "struct {};", cpp_arg(index)).unwrap();
    }
    for name in &deprecated {
        if let Some(replacement) = &name.replacement {
            ensure!(
                cpp_params(&name.name) == cpp_params(replacement),
                "{}: `{}` and `{replacement}` take different numbers of template parameters",
                name.role,
                name.name
            );
            let (old, new) = (cpp_type(name, &name.name), cpp_type(name, replacement));
            writeln!(out, "static_assert(std::is_same_v<{old}, {new}>);").unwrap();
        }
    }
    out.push_str("#pragma GCC diagnostic pop\n");
    Ok(out)
}

fn outputs(names: &[Name]) -> Result<Vec<Output>> {
    let rust_module_prelude = concat!(
        "    #[spacetimedb::table(accessor = sdk_names_row)]\n",
        "    pub struct SdkNamesRow {\n",
        "        #[primary_key]\n",
        "        id: u32,\n",
        "    }\n\n",
    );
    let rust_module = names.iter().filter(|n| n.column == Column::RustModule);
    Ok(vec![
        Output {
            path: "crates/bindings/tests/sdk_names.rs",
            contents: rust_uses(rust_module, rust_module_prelude, "spacetimedb", "self"),
        },
        Output {
            path: "sdks/rust/tests/sdk_names.rs",
            contents: rust_uses(names_in(names, Column::RustClient, false), "", "spacetimedb_sdk", ""),
        },
        Output {
            path: "sdks/rust/tests/view-client/src/sdk_names.rs",
            contents: rust_uses(
                names_in(names, Column::RustClient, true),
                "",
                "crate::module_bindings",
                "crate::module_bindings",
            ),
        },
        Output {
            path: "crates/bindings-typescript/src/lib/sdk_names_module.test-d.ts",
            contents: ts_imports(names_in(names, Column::TsModule, false), |n| {
                ts_sdk_specifier(n, "spacetimedb/server")
            })?,
        },
        Output {
            path: "crates/bindings-typescript/src/lib/sdk_names_client.test-d.ts",
            contents: ts_imports(names_in(names, Column::TsClient, false), |n| {
                ts_sdk_specifier(n, "spacetimedb")
            })?,
        },
        Output {
            path: "crates/bindings-typescript/test-app/src/sdk_names.test-d.ts",
            contents: ts_imports(names_in(names, Column::TsClient, true), |n| match &n.path {
                Some(path) => format!("./module_bindings/{path}"),
                None => "./module_bindings".to_owned(),
            })?,
        },
        Output {
            path: "crates/bindings-csharp/Codegen.Tests/fixtures/server/SdkNames.g.cs",
            contents: csharp_typeofs(
                names.iter().filter(|n| n.column == Column::CsharpModule),
                concat!(
                    "\n// The module runtime and the module generator both define\n",
                    "// SpacetimeDB.Internal.LocalReadOnly, so naming it warns that one hides the other\n",
                    "// (CS0436). This file checks only that names resolve.\n",
                    "#pragma warning disable CS0436\n",
                ),
                "SpacetimeDB",
                "SpacetimeDB",
            ),
        },
        Output {
            path: "sdks/csharp/examples~/regression-tests/client/SdkNames.g.cs",
            contents: csharp_typeofs(
                names.iter().filter(|n| n.column == Column::CsharpClient),
                "",
                "SpacetimeDB",
                "SpacetimeDB.Types",
            ),
        },
        Output {
            path: "crates/bindings-cpp/tests/compile/cases/indexes/ok_sdk_names.cpp",
            contents: cpp_case(names.iter().filter(|n| n.column == Column::CppModule))?,
        },
    ])
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A preprocessor conditional, from `#if` to `#endif`, that encloses the current line.
struct Conditional {
    /// Whether the code around the conditional is live.
    outer: bool,
    /// Whether an earlier branch's condition is true, so that later branches are dead.
    taken: bool,
    /// Whether the current branch is live.
    live: bool,
}

impl Conditional {
    /// Starts the next branch, whose condition is true, false, or unknown (`None`).
    fn branch(&mut self, condition: Option<bool>) {
        self.live = self.outer && !self.taken && condition != Some(false);
        self.taken |= condition == Some(true);
    }
}

/// The value of a conditional directive's condition, if it is literally `0` or `1` (or `false`
/// or `true`), in any parentheses. Any other condition may be either.
fn cpp_condition(mut tokens: &[&str]) -> Option<bool> {
    while let ["(", inner @ .., ")"] = tokens {
        tokens = inner;
    }
    match tokens {
        ["0" | "false"] => Some(false),
        ["1" | "true"] => Some(true),
        _ => None,
    }
}

/// C++ source without comments, string and character literals, or dead preprocessor branches, so
/// that only live code counts. A branch is dead if its condition is literally `0`, if an earlier
/// branch of the same conditional has a condition that is literally `1`, or if it is inside a
/// dead branch. A branch with any other condition counts as live.
fn cpp_code(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut code = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let (c, next) = (chars[i], chars.get(i + 1).copied());
        // A quote between digits, as in 1'000, separates digits.
        let separator =
            c == '\'' && i > 0 && chars[i - 1].is_ascii_digit() && next.is_some_and(|n| n.is_ascii_hexdigit());
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            // A comment is a space, so `#if/**/0` is `#if 0`.
            code.push(' ');
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                if chars[i] == '\n' {
                    code.push('\n');
                }
                i += 1;
            }
            i += 2;
        } else if c == '"' || (c == '\'' && !separator) {
            i += 1;
            while i < chars.len() && chars[i] != c && chars[i] != '\n' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i += 1;
            code.push(' ');
        } else {
            code.push(c);
            i += 1;
        }
    }
    let mut live = String::with_capacity(code.len());
    // The conditionals that enclose the current line, innermost last.
    let mut conditionals: Vec<Conditional> = Vec::new();
    for line in code.lines() {
        // Whether the current line is in live code.
        let live_here = conditionals.last().is_none_or(|c| c.live);
        match cpp_tokens(line).as_slice() {
            ["#", directive @ ("if" | "ifdef" | "ifndef"), condition @ ..] => {
                let mut conditional = Conditional {
                    outer: live_here,
                    taken: false,
                    live: false,
                };
                conditional.branch(if *directive == "if" {
                    cpp_condition(condition)
                } else {
                    None
                });
                conditionals.push(conditional);
            }
            ["#", directive @ ("elif" | "elifdef" | "elifndef" | "else"), condition @ ..] => {
                if let Some(conditional) = conditionals.last_mut() {
                    conditional.branch(match *directive {
                        "elif" => cpp_condition(condition),
                        "else" => Some(true),
                        _ => None,
                    });
                }
            }
            ["#", "endif", ..] => {
                conditionals.pop();
            }
            _ if live_here => {
                live.push_str(line);
                live.push('\n');
            }
            _ => {}
        }
    }
    live
}

/// The identifier and punctuation tokens of C++ code.
fn cpp_tokens(code: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    for line in code.lines() {
        let mut start = None;
        for (i, c) in line.char_indices() {
            match (is_ident(c), start) {
                (true, None) => start = Some(i),
                (true, Some(_)) => {}
                (false, s) => {
                    if let Some(s) = s {
                        tokens.push(&line[s..i]);
                    }
                    start = None;
                    if !c.is_whitespace() {
                        tokens.push(&line[i..i + c.len_utf8()]);
                    }
                }
            }
        }
        if let Some(s) = start {
            tokens.push(&line[s..]);
        }
    }
    tokens
}

/// The classes, structs, and enums that C++ source defines (not only declares) in live code.
fn cpp_definitions(text: &str, out: &mut BTreeSet<String>) {
    let code = cpp_code(text);
    let tokens = cpp_tokens(&code);
    for (i, &token) in tokens.iter().enumerate() {
        if !matches!(token, "class" | "struct") || (i > 0 && tokens[i - 1] == "friend") {
            continue;
        }
        let mut j = i + 1;
        if tokens.get(j).is_some_and(|t| t.ends_with("_API")) {
            j += 1;
        }
        if let (Some(name), Some(&"{" | &":" | &"final")) = (tokens.get(j), tokens.get(j + 1)) {
            out.insert((*name).to_owned());
        }
    }
}

/// The contents of the string literals in Rust source, one per line, with escapes decoded and
/// without comments.
fn rust_strings(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let (c, next) = (chars[i], chars.get(i + 1).copied());
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                let pair = (chars[i], chars.get(i + 1).copied());
                i += if matches!(pair, ('/', Some('*')) | ('*', Some('/'))) {
                    2
                } else {
                    1
                };
                depth += match pair {
                    ('/', Some('*')) => 1,
                    ('*', Some('/')) => -1,
                    _ => 0,
                };
                if depth == 0 {
                    break;
                }
            }
        } else if c == '\'' {
            // A character literal, or a lifetime or label, which has no closing quote.
            i += match (next, chars.get(i + 2)) {
                (Some('\\'), _) => 2 + chars[i + 2..].iter().position(|&c| c == '\'').map_or(0, |p| p + 1),
                (_, Some('\'')) => 3,
                _ => 1,
            };
        } else if is_ident(c) {
            let start = i;
            while i < chars.len() && is_ident(chars[i]) {
                i += 1;
            }
            let prefix: String = chars[start..i].iter().collect();
            let hashes = chars[i..].iter().take_while(|&&c| c == '#').count();
            if matches!(prefix.as_str(), "r" | "br" | "cr") && chars.get(i + hashes) == Some(&'"') {
                // A raw string, which ends at a quote followed by as many `#` as it starts with.
                let close: String = std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
                let rest: String = chars[i + hashes + 1..].iter().collect();
                let len = rest.find(&close).unwrap_or(rest.len());
                out.push_str(&rest[..len]);
                out.push('\n');
                i += hashes + 1 + rest[..len].chars().count() + close.chars().count();
            }
        } else if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                    match chars.get(i) {
                        // A line continuation skips the next line's indentation.
                        Some('\n') => {
                            while chars.get(i + 1).is_some_and(|c| c.is_whitespace()) {
                                i += 1;
                            }
                        }
                        Some('n' | 't' | 'r' | '0' | 'u' | 'x') => out.push(' '),
                        Some(&escaped) => out.push(escaped),
                        None => {}
                    }
                } else {
                    out.push(chars[i]);
                }
                i += 1;
            }
            i += 1;
            out.push('\n');
        } else {
            i += 1;
        }
    }
    out
}

/// Replaces every `{...}` format argument or placeholder with `{}`.
fn erase_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let len = after.find(|c: char| !is_ident(c)).unwrap_or(after.len());
        if after[len..].starts_with('}') {
            out.push_str("{}");
            rest = &after[len + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Whether `needle` occurs in `haystack` as a whole identifier.
fn contains_ident(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + needle.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(|c| is_ident(c) || c == '{')
    })
}

fn walk(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, extension, files)?;
        } else if path.extension().is_some_and(|e| e == extension) {
            files.push(path);
        }
    }
    Ok(())
}

/// CI does not compile Unreal code, so Unreal names are checked textually: an SDK name must be
/// defined as a class or struct in live code (not in a comment, a string, or a dead preprocessor
/// branch such as `#if 0`) of the SDK's headers, and a generated name must appear in a string
/// literal of the Unreal codegen, where `{module_prefix}` is empty and any other `{...}` matches
/// any placeholder.
fn check_unreal(root: &Path, names: &[Name], problems: &mut Vec<String>) -> Result<()> {
    let mut headers = Vec::new();
    walk(&root.join("sdks/unreal/src"), "h", &mut headers)?;
    let mut defined = BTreeSet::new();
    for header in headers {
        cpp_definitions(&fs::read_to_string(&header)?, &mut defined);
    }
    let codegen_path = "crates/codegen/src/unrealcpp.rs";
    let codegen = rust_strings(&fs::read_to_string(root.join(codegen_path))?);
    let codegen = erase_placeholders(&codegen.replace("{module_prefix}", ""));
    for name in names.iter().filter(|n| n.column == Column::Unreal) {
        if name.generated {
            if !contains_ident(&codegen, &erase_placeholders(&name.name)) {
                problems.push(format!(
                    "{}: no string literal in Unreal codegen ({codegen_path}) emits `{}`",
                    name.role, name.name
                ));
            }
        } else if !defined.contains(&name.name) {
            problems.push(format!(
                "{}: no header under sdks/unreal/src defines `{}`",
                name.role, name.name
            ));
        }
    }
    Ok(())
}

/// The words of a name, lowercased, so that casing conventions compare equal: `ReadOnlyDbView`
/// and `read_only_db_view` both give `read only db view`. A placeholder such as `{Table}` is one
/// word.
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let (mut words, mut word, mut in_placeholder) = (Vec::new(), String::new(), false);
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { chars[i - 1] } else { ' ' };
        let next_lower = chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
        let starts_word = c.is_ascii_uppercase()
            && !in_placeholder
            && (prev.is_ascii_lowercase() || prev.is_ascii_digit() || (prev.is_ascii_uppercase() && next_lower));
        if (c == '{' || c == '_' || starts_word) && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        in_placeholder |= c == '{';
        if c != '_' {
            word.push(c.to_ascii_lowercase());
        }
        if c == '}' {
            in_placeholder = false;
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The words of a name in `column`, after undoing that language's rendering rules: take the last
/// segment of a nested name, drop generic parameters, drop C#'s `I` on an interface, drop
/// Unreal's `U`, `F`, or `E`, module prefix, and `SpacetimeDB` prefix, and read TypeScript's
/// `Readonly` as `ReadOnly`.
fn scheme_words(column: Column, name: &str) -> Vec<String> {
    let name = name.split('<').next().unwrap();
    let mut name = name.rsplit('.').next().unwrap().to_owned();
    let mut strip = |prefix: &str| {
        let rest = name
            .strip_prefix(prefix)
            .filter(|r| r.starts_with(|c: char| c.is_ascii_uppercase() || c == '{'));
        if let Some(rest) = rest {
            name = rest.to_owned();
        }
    };
    match column {
        Column::CsharpModule | Column::CsharpClient => strip("I"),
        Column::Unreal => ["U", "F", "E", "{Module}", "SpacetimeDB"].into_iter().for_each(strip),
        Column::TsModule | Column::TsClient => {
            if let Some(rest) = name.strip_prefix("Readonly") {
                name = format!("ReadOnly{rest}");
            }
        }
        _ => {}
    }
    words(&name)
}

/// Checks that each current name follows its role's scheme name, after its language's rendering
/// rules, or has a deviation, and that no name with a deviation follows it. An SDK type is not
/// specific to one item, so an SDK name is compared with the scheme name without placeholders.
fn check_scheme(manifest: &Manifest, problems: &mut Vec<String>) {
    for role in &manifest.role {
        let Some(scheme) = &role.name else { continue };
        for column in Column::ALL {
            let Cell::Item(item) = role.cell(column) else { continue };
            let Some(name) = item.sdk.as_ref().or(item.generated.as_ref()) else {
                continue;
            };
            let expected: Vec<_> = words(scheme)
                .into_iter()
                .filter(|w| item.generated.is_some() || !w.starts_with('{'))
                .collect();
            let key = column.key();
            match (scheme_words(column, name) == expected, role.deviations.contains_key(key)) {
                (false, false) => problems.push(format!(
                    "{}: {key} name `{name}` does not follow the scheme name `{scheme}`; rename it or record a deviation",
                    role.id
                )),
                (true, true) => problems.push(format!(
                    "{}: {key} name `{name}` follows the scheme name `{scheme}`, so it needs no deviation",
                    role.id
                )),
                _ => {}
            }
        }
    }
}

/// The lines that register each generated test with the build that runs it: the file, the line,
/// and, for an element of a list, the end of the line that opens the list. Lines are compared
/// trimmed and without comments, so a line that is commented out does not count.
const WIRING: &[(&str, &str, Option<&str>)] = &[
    ("sdks/rust/tests/view-client/src/lib.rs", "mod sdk_names;", None),
    (
        "crates/bindings-cpp/tests/compile/run-compile-tests.sh",
        "\"ok_sdk_names\"",
        Some("CASE_NAMES=("),
    ),
    (
        "crates/bindings-cpp/tests/compile/run-compile-tests.sh",
        "CASE_SOURCE[\"ok_sdk_names\"]=\"$SCRIPT_DIR/cases/indexes/ok_sdk_names.cpp\"",
        None,
    ),
    (
        "crates/bindings-cpp/tests/compile/run-compile-tests.ps1",
        "(New-CompileCase \"ok_sdk_names\" \"cases/indexes/ok_sdk_names.cpp\" \"success\")",
        Some("@("),
    ),
];

/// The index after the quoted literal that starts at `chars[start]`, in which `escape`, if given,
/// escapes the next character.
fn quoted_end(chars: &[char], start: usize, escape: Option<char>) -> usize {
    let mut i = start + 1;
    while i < chars.len() && chars[i] != chars[start] {
        i += if Some(chars[i]) == escape { 2 } else { 1 };
    }
    (i + 1).min(chars.len())
}

/// `text` without comments, in the language of the file at `path`: `//` and nested `/* */` in
/// Rust (`.rs`), a `#` that starts a word in bash (`.sh`) and PowerShell (`.ps1`), and `<# #>` in
/// PowerShell. String and character literals are kept. A block comment becomes a space but keeps
/// its line breaks, so that the code after it stays on its own line.
fn strip_comments(path: &str, text: &str) -> String {
    let (rust, powershell) = (path.ends_with(".rs"), path.ends_with(".ps1"));
    assert!(
        rust || powershell || path.ends_with(".sh"),
        "no comment syntax for {path}"
    );
    let escape = if powershell { '`' } else { '\\' };
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let (c, next) = (chars[i], chars.get(i + 1).copied());
        let starts_word = i == 0 || chars[i - 1].is_whitespace() || ";&|()".contains(chars[i - 1]);
        if (rust && c == '/' && next == Some('/')) || (!rust && c == '#' && starts_word) {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if (rust && c == '/' && next == Some('*')) || (powershell && c == '<' && next == Some('#')) {
            // Rust's block comments nest, and PowerShell's don't.
            let close = if rust { ['*', '/'] } else { ['#', '>'] };
            let mut depth = 1;
            out.push(' ');
            i += 2;
            while i < chars.len() && depth > 0 {
                let pair = [chars[i], chars.get(i + 1).copied().unwrap_or(' ')];
                if pair == close || (rust && pair == ['/', '*']) {
                    depth += if pair == close { -1 } else { 1 };
                    i += 2;
                } else {
                    if chars[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
            continue;
        }
        let end = if rust && c == '\'' {
            // A character literal, or a lifetime or label, which has no closing quote.
            match (next, chars.get(i + 2)) {
                (Some('\\'), _) => quoted_end(&chars, i, Some('\\')),
                (Some(_), Some('\'')) => i + 3,
                _ => i + 1,
            }
        } else if c == '"' || c == '\'' {
            // A single-quoted shell string has no escapes.
            quoted_end(&chars, i, (c == '"').then_some(escape))
        } else if rust && is_ident(c) {
            // An identifier, or a raw string, which ends at a quote followed by as many `#` as
            // it starts with.
            let mut end = i;
            while end < chars.len() && is_ident(chars[end]) {
                end += 1;
            }
            let prefix: String = chars[i..end].iter().collect();
            let hashes = chars[end..].iter().take_while(|&&c| c == '#').count();
            if matches!(prefix.as_str(), "r" | "br" | "cr") && chars.get(end + hashes) == Some(&'"') {
                let close: Vec<char> = std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
                let body = end + hashes + 1;
                let len = chars[body..].windows(close.len()).position(|w| w == close);
                end = len.map_or(chars.len(), |len| body + len + close.len());
            }
            end
        } else if !rust && c == escape {
            (i + 2).min(chars.len())
        } else {
            i + 1
        };
        out.extend(&chars[i..end]);
        i = end;
    }
    out
}

/// Whether `text`, from the file at `path`, has `line` outside comments, inside a list that a
/// line ending in `list` opens, if given.
fn registered(path: &str, text: &str, line: &str, list: Option<&str>) -> bool {
    let mut in_list = false;
    strip_comments(path, text).lines().map(str::trim).any(|l| {
        if list.is_some_and(|open| l.ends_with(open)) {
            in_list = true;
        } else if l == ")" {
            in_list = false;
        }
        l == line && (list.is_none() || in_list)
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = ci_common::repo_root();
    let manifest = fs::read_to_string(root.join(MANIFEST)).with_context(|| format!("reading {MANIFEST}"))?;
    let manifest: Manifest = toml::from_str(&manifest).with_context(|| format!("parsing {MANIFEST}"))?;
    validate(&manifest)?;
    let names = collect_names(&manifest)?;

    let mut problems = Vec::new();
    check_scheme(&manifest, &mut problems);
    check_unreal(&root, &names, &mut problems)?;
    for (file, line, list) in WIRING {
        if !registered(file, &fs::read_to_string(root.join(file))?, line, *list) {
            let place = list.map_or(String::new(), |open| format!(" in the list that `{open}` opens"));
            problems.push(format!(
                "{file} must have the line `{line}`{place}, so that CI runs the generated test"
            ));
        }
    }

    let mut stale = Vec::new();
    for output in outputs(&names)? {
        let path = root.join(output.path);
        let existing = fs::read_to_string(&path).ok().map(|s| s.replace("\r\n", "\n"));
        if existing.as_deref() == Some(output.contents.as_str()) {
            continue;
        }
        if !cli.check {
            fs::write(&path, &output.contents).with_context(|| format!("writing {}", path.display()))?;
        }
        stale.push(output.path);
    }
    if !stale.is_empty() {
        let action = if cli.check {
            "run `cargo ci sdk-names` to rewrite them"
        } else {
            "they have been rewritten, so commit them"
        };
        problems.push(format!(
            "these generated tests were out of date with {MANIFEST}; {action}:\n  {}",
            stale.join("\n  ")
        ));
    }

    if !problems.is_empty() {
        bail!("SDK name check failed:\n{}", problems.join("\n"));
    }
    println!("{} names in {} roles are up to date.", names.len(), manifest.role.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_registration_ignores_comments() {
        let registered = |text| registered("lib.rs", text, "mod sdk_names;", None);
        assert!(registered("mod sdk_names; // the conformance test\n"));
        assert!(registered("/* a */ mod sdk_names;\n"));
        // Comment markers in literals don't start comments.
        assert!(registered(
            "const A: &str = \"/*\";\nmod sdk_names;\nconst B: &str = \"*/\";\n"
        ));
        assert!(registered(
            "const A: &str = r#\"\"/*\"#;\nmod sdk_names;\nconst B: &str = \"*/\";\n"
        ));
        assert!(!registered("// mod sdk_names;\n"));
        assert!(!registered("/*\nmod sdk_names;\n*/\n"));
        assert!(!registered("/* /* */\nmod sdk_names;\n*/\n"));
        assert!(!registered("const Q: char = '\"'; /*\nmod sdk_names;\n*/\n"));
        assert!(!registered("const Q: char = '\\''; /*\nmod sdk_names;\n*/\n"));
        assert!(!registered("fn f(_: &'static str) {} /*\nmod sdk_names;\n*/\n"));
    }

    #[test]
    fn bash_registration_ignores_comments() {
        let registered = |text| registered("run.sh", text, "\"ok\"", Some("CASE_NAMES=("));
        assert!(registered("CASE_NAMES=(\n    \"ok\" # the conformance test\n)\n"));
        // A `#` inside a word, a quote, or an escaped quote doesn't start a comment or hide one.
        assert!(registered("echo $# a#b \"it's\"; CASE_NAMES=(\n    \"ok\" # note\n)\n"));
        assert!(registered(
            "echo \\\"; CASE_NAMES=(\n    \"ok\" # note\n)\necho \"'\"\n"
        ));
        assert!(registered(
            "echo \"\\\"'\"; CASE_NAMES=(\n    \"ok\" # note\n)\necho \"'\"\n"
        ));
        assert!(!registered("CASE_NAMES=(\n    # \"ok\"\n)\n"));
        assert!(!registered("# CASE_NAMES=(\n    \"ok\"\n)\n"));
    }

    #[test]
    fn powershell_registration_ignores_comments() {
        let registered = |text| registered("run.ps1", text, "(Case \"ok\")", Some("@("));
        assert!(registered("@(\n    (Case \"ok\") # the conformance test\n)\n"));
        // A `#` inside a word, a quote, or an escaped quote doesn't start a comment or hide one.
        assert!(registered("Write-Host a#b; $cases = @(\n    (Case \"ok\")\n)\n"));
        assert!(registered(
            "Write-Host `\"\n@(\n    (Case \"ok\") # note\n)\nWrite-Host \"'\"\n"
        ));
        assert!(registered(
            "Write-Host \"`\" <#\"\n@(\n    (Case \"ok\")\n)\nWrite-Host \"#>\"\n"
        ));
        assert!(!registered("@(\n    # (Case \"ok\")\n)\n"));
        assert!(!registered("@(\n<#\n    (Case \"ok\")\n#>\n)\n"));
        assert!(!registered("@(\n    <# (Case \"ok\") #>\n)\n"));
    }

    fn definitions(text: &str) -> Vec<String> {
        let mut out = BTreeSet::new();
        cpp_definitions(text, &mut out);
        out.into_iter().collect()
    }

    #[test]
    fn unreal_scanner_skips_dead_branches() {
        let text = "\
#if 0
struct A {};
#elif 0
struct B {};
#elif WITH_EDITOR
struct C {};
#else
struct D {};
#endif
#if 1
struct E {};
#elif FOO
struct F {};
#else
struct G {};
#endif
#if 0
struct H {};
#else
struct I {};
#endif
#if 0
#if FOO
struct J {};
#else
struct K {};
#endif
#else
#ifdef BAR
struct L {};
#elif 0
struct M {};
#endif
#endif
struct N {};
";
        assert_eq!(definitions(text), ["C", "D", "E", "I", "L", "N"]);
    }

    #[test]
    fn unreal_scanner_tokenizes_directives() {
        for directive in [
            "#if  0",
            "# if 0",
            "#\tif\t0",
            "  #  if 0",
            "#if (0)",
            "#if/**/0",
            "#if 0 // off",
        ] {
            let text = format!("{directive}\nstruct A {{}};\n#endif\nstruct B {{}};\n");
            assert_eq!(definitions(&text), ["B"], "{directive}");
        }
    }

    fn cpp_name(name: &str, replacement: Option<&str>) -> Name {
        Name {
            role: "role".to_owned(),
            column: Column::CppModule,
            generated: false,
            path: None,
            name: name.to_owned(),
            deprecated: replacement.is_some(),
            replacement: replacement.map(str::to_owned),
            cfg: None,
            value: false,
        }
    }

    #[test]
    fn cpp_case_passes_distinct_template_arguments() {
        let names = [cpp_name("New<T, F>", None), cpp_name("Old<T, F>", Some("New<T, F>"))];
        let case = cpp_case(names.iter()).unwrap();
        assert!(case.contains("struct SdkNamesArg0;\nstruct SdkNamesArg1;\n"));
        assert!(case.contains(
            "static_assert(std::is_same_v<SpacetimeDB::Old<SdkNamesArg0, SdkNamesArg1>, SpacetimeDB::New<SdkNamesArg0, SdkNamesArg1>>);"
        ));

        // Without templates, the case declares no argument types.
        let names = [cpp_name("New", None), cpp_name("Old", Some("New"))];
        let case = cpp_case(names.iter()).unwrap();
        assert!(!case.contains("SdkNamesArg"));
        assert!(case.contains("static_assert(std::is_same_v<SpacetimeDB::Old, SpacetimeDB::New>);"));
    }
}
