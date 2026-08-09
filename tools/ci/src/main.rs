#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use duct::cmd;
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
        package: "ci-dlls",
    },
    Command {
        name: "smoketests",
        package: "ci-smoketests",
    },
    Command {
        name: "keynote-bench",
        package: "ci-keynote-bench",
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

    let readme = generate_cli_docs()?;
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

fn normalize_help(help: &str) -> String {
    help.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

fn command_help_safe(name: &str) -> bool {
    matches!(
        name,
        "smoketests" | "update-flow" | "cli-docs" | "codeowners-check" | "coordinate-internal-tests" | "cla-assistant"
    )
}

fn command_usage(command: &Command) -> Result<String> {
    if command.name == "self-docs" {
        return Ok("cargo ci self-docs [--check]".to_owned());
    }
    if command_help_safe(command.name) {
        let help = cmd!("cargo", "run", "--quiet", "--package", command.package, "--", "--help").read()?;
        return Ok(normalize_help(&help));
    }
    Ok(format!("cargo ci {}", command.name))
}

fn package_usage(package: &str, args: &[&str]) -> Result<String> {
    let help = cmd(
        "cargo",
        ["run", "--quiet", "--package", package, "--"]
            .into_iter()
            .chain(args.iter().copied()),
    )
    .read()?;
    Ok(normalize_help(&help))
}

fn generate_cli_docs() -> Result<String> {
    let mut out = String::from("# SpacetimeDB's cargo ci\n\n## Overview\n\nThis document provides an overview of the `cargo ci` command-line tool, and documentation for each of its subcommands and options.\n\n## `cargo ci`\n\n**Usage:**\n```bash\ncargo ci [--skip <COMMAND>] [COMMAND] [ARGS]...\n```\n\n");
    for command in COMMANDS {
        out.push_str(&format!(
            "### `{}`\n\n**Usage:**\n```bash\n{}\n```\n\n",
            command.name,
            command_usage(command)?.trim(),
        ));
    }
    for group in COMMAND_GROUPS {
        out.push_str(&format!(
            "### `{}`\n\n**Usage:**\n```bash\ncargo ci {} <COMMAND>\n```\n\n",
            group.name, group.name,
        ));
        for command in group.commands {
            out.push_str(&format!(
                "#### `{}`\n\n**Usage:**\n```bash\n{}\n```\n\n",
                command.name,
                command_usage(command)?.trim(),
            ));
            if command.name == "cla-assistant" {
                for subcommand in ["retry", "status"] {
                    out.push_str(&format!(
                        "##### `{}`\n\n**Usage:**\n```bash\n{}\n```\n\n",
                        subcommand,
                        package_usage(command.package, &[subcommand, "--help"])?.trim(),
                    ));
                }
            }
        }
    }
    out.push_str("---\n\nThis document is auto-generated by running:\n\n```bash\ncargo ci self-docs\n```\n");
    Ok(out)
}
