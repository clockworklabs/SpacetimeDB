#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use duct::cmd;
use keynote_bench_harness::KeynoteBenchConfig;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use std::fs;
use std::path::Path;

struct Command {
    name: &'static str,
    package: &'static str,
}
struct CommandGroup {
    name: &'static str,
    commands: &'static [Command],
}
const OTHER_WORKFLOWS_COMMANDS: &[Command] = &[
    Command {
        name: "coordinate-internal-tests",
        package: "ci-coordinate-internal-tests",
    },
    Command {
        name: "codeowners-check",
        package: "ci-codeowners-check",
    },
    Command {
        name: "cla-assistant",
        package: "ci-cla-assistant",
    },
];
const COMMANDS: &[Command] = &[
    Command {
        name: "test",
        package: "ci-test",
    },
    Command {
        name: "lint",
        package: "ci-lint",
    },
    Command {
        name: "wasm-bindings",
        package: "ci-wasm-bindings",
    },
    Command {
        name: "dlls",
        package: "",
    },
    Command {
        name: "smoketests",
        package: "ci-smoketests",
    },
    Command {
        name: "keynote-bench",
        package: "",
    },
    Command {
        name: "update-flow",
        package: "ci-update-flow",
    },
    Command {
        name: "cli-docs",
        package: "ci-cli-docs",
    },
    Command {
        name: "self-docs",
        package: "",
    },
    Command {
        name: "global-json-policy",
        package: "ci-global-json-policy",
    },
    Command {
        name: "publish-checks",
        package: "ci-publish-checks",
    },
    Command {
        name: "typescript-test",
        package: "ci-typescript-test",
    },
    Command {
        name: "version-upgrade-check",
        package: "ci-version-upgrade-check",
    },
    Command {
        name: "docs",
        package: "ci-docs-build",
    },
];
const COMMAND_GROUPS: &[CommandGroup] = &[CommandGroup {
    name: "other-workflows",
    commands: OTHER_WORKFLOWS_COMMANDS,
}];
const DEFAULT_SKIP: &[&str] = &["other-workflows"];

fn print_help() {
    println!("Usage: cargo ci [--skip <COMMAND>] [COMMAND] [ARGS]...");
    println!();
    println!("Commands:");
    for command in COMMANDS {
        println!("  {}", command.name);
    }
    for group in COMMAND_GROUPS {
        println!("  {}", group.name);
    }
}
fn command_for(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|candidate| candidate.name == name)
}
fn command_group_for(name: &str) -> Option<&'static CommandGroup> {
    COMMAND_GROUPS.iter().find(|candidate| candidate.name == name)
}
fn run_command(command: &Command, forwarded: &[String]) -> Result<()> {
    if command.name == "self-docs" {
        return run_self_docs(forwarded);
    }
    if command.name == "dlls" {
        return run_dlls(forwarded);
    }
    if command.name == "keynote-bench" {
        return run_keynote_bench(forwarded);
    }
    let mut cargo_args = vec!["run", "--package", command.package, "--"];
    cargo_args.extend(forwarded.iter().map(String::as_str));
    cmd("cargo", cargo_args).run()?;
    Ok(())
}
fn main() -> Result<()> {
    let mut raw = std::env::args().skip(1).collect::<Vec<_>>();
    if raw.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }
    let mut skips = DEFAULT_SKIP.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == "--skip" {
            if i + 1 >= raw.len() {
                bail!("--skip requires a command name");
            }
            skips.push(raw.remove(i + 1));
            raw.remove(i);
        } else {
            i += 1;
        }
    }
    if raw.is_empty() {
        for command in COMMANDS {
            if !skips.iter().any(|skip| skip == command.name) {
                run_command(command, &[])?;
            }
        }
        return Ok(());
    }
    let command_name = raw.remove(0);
    if let Some(group) = command_group_for(&command_name) {
        return run_command_group(group, raw);
    }
    let Some(command) = command_for(&command_name) else {
        bail!("unknown cargo ci command `{command_name}`");
    };
    run_command(command, &raw)
}
fn run_command_group(group: &CommandGroup, mut forwarded: Vec<String>) -> Result<()> {
    if forwarded.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        println!("Usage: cargo ci {} <COMMAND>", group.name);
        println!();
        println!("Commands:");
        for command in group.commands {
            println!("  {}", command.name);
        }
        return Ok(());
    }
    if forwarded.is_empty() {
        bail!("cargo ci {} requires a command name", group.name);
    }
    let command_name = forwarded.remove(0);
    let Some(command) = group.commands.iter().find(|candidate| candidate.name == command_name) else {
        bail!("unknown cargo ci {} command `{command_name}`", group.name);
    };
    run_command(command, &forwarded)
}

fn run_self_docs(args: &[String]) -> Result<()> {
    if args.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        println!("Usage: cargo ci self-docs [--check]");
        println!();
        println!("Options:");
        println!("      --check  Only check for changes, do not generate the docs");
        println!("  -h, --help   Print help");
        return Ok(());
    }

    let check = match args {
        [] => false,
        [arg] if arg == "--check" => true,
        _ => bail!("cargo ci self-docs accepts only --check"),
    };

    let readme = generate_cli_docs();
    let path = Path::new("tools/ci/README.md");
    if check {
        let existing = fs::read_to_string(path).unwrap_or_default();
        if existing != readme {
            bail!("README.md is out of date. Please run `cargo ci self-docs` to update it.");
        }
    } else {
        fs::write(path, readme)?;
    }
    Ok(())
}

fn run_dlls(args: &[String]) -> Result<()> {
    if args.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        println!("Usage: cargo ci dlls");
        return Ok(());
    }
    if !args.is_empty() {
        bail!("cargo ci dlls does not accept arguments");
    }
    eprintln!("warning: `cargo ci dlls` is deprecated; use `cargo regen csharp dlls` instead");
    cmd!("cargo", "regen", "csharp", "dlls").run()?;
    Ok(())
}

fn run_keynote_bench(args: &[String]) -> Result<()> {
    if args.first().is_some_and(|arg| arg == "-h" || arg == "--help") {
        println!("Usage: cargo ci keynote-bench");
        return Ok(());
    }
    if !args.is_empty() {
        bail!("cargo ci keynote-bench does not accept arguments");
    }

    let cli_path = ensure_binaries_built();
    let server = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let server_url = server.host_url.clone();

    keynote_bench_harness::run(KeynoteBenchConfig::standalone(".", cli_path, server_url))
}

fn generate_cli_docs() -> String {
    include_str!("../README.md").to_owned()
}
