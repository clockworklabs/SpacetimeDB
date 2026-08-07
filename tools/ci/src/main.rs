#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use duct::cmd;

struct Command {
    name: &'static str,
    package: &'static str,
}
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
        package: "ci-self-docs",
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
    Command {
        name: "other-workflows",
        package: "ci-other-workflows",
    },
];
const DEFAULT_SKIP: &[&str] = &["other-workflows"];

fn print_help() {
    println!("Usage: cargo ci [--skip <COMMAND>] [COMMAND] [ARGS]...");
    println!();
    println!("Commands:");
    for command in COMMANDS {
        println!("  {}", command.name);
    }
}
fn command_for(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|candidate| candidate.name == name)
}
fn run_command(command: &Command, forwarded: &[String]) -> Result<()> {
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
    let Some(command) = command_for(&command_name) else {
        bail!("unknown cargo ci command `{command_name}`");
    };
    run_command(command, &raw)
}
