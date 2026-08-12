#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use duct::cmd;

struct Command {
    path: &'static [&'static str],
    package: Option<&'static str>,
}

// Each of these commands is in its own binary package, in order to keep the
// dependencies of each one as minimal as possible. Do not add new business
// logic commands that are not in their own binaries.
const COMMANDS: &[Command] = &[
    Command {
        path: &["test"],
        package: Some("ci-test"),
    },
    Command {
        path: &["lint"],
        package: Some("ci-lint"),
    },
    Command {
        path: &["wasm-bindings"],
        package: Some("ci-wasm-bindings"),
    },
    Command {
        path: &["dlls"],
        package: None,
    },
    Command {
        path: &["smoketests"],
        package: Some("ci-smoketests"),
    },
    Command {
        path: &["keynote-bench"],
        package: Some("ci-keynote-bench"),
    },
    Command {
        path: &["update-flow"],
        package: Some("ci-update-flow"),
    },
    Command {
        path: &["cli-docs"],
        package: Some("ci-cli-docs"),
    },
    Command {
        path: &["global-json-policy"],
        package: Some("ci-global-json-policy"),
    },
    Command {
        path: &["publish-checks"],
        package: Some("ci-publish-checks"),
    },
    Command {
        path: &["typescript-test"],
        package: Some("ci-typescript-test"),
    },
    Command {
        path: &["version-upgrade-check"],
        package: Some("ci-version-upgrade-check"),
    },
    Command {
        path: &["docs"],
        package: Some("ci-docs-build"),
    },
    Command {
        path: &["other-workflows", "coordinate-internal-tests"],
        package: Some("ci-coordinate-internal-tests"),
    },
    Command {
        path: &["other-workflows", "codeowners-check"],
        package: Some("ci-codeowners-check"),
    },
    Command {
        path: &["other-workflows", "cla-assistant"],
        package: Some("ci-cla-assistant"),
    },
    Command {
        path: &["other-workflows", "watch"],
        package: Some("ci-workflow-watch"),
    },
];

fn print_help() {
    println!("Usage: cargo ci [--skip <COMMAND>...]");
    println!("       cargo ci <COMMAND> [ARGS]...");
    println!();
    println!("Commands:");
    for command in COMMANDS {
        println!("  {}", command.path.join(" "));
    }
}

fn command_for(args: &[String]) -> Option<(&'static Command, usize)> {
    COMMANDS
        .iter()
        .filter_map(|command| {
            args.get(..command.path.len())
                .is_some_and(|head| head.iter().map(String::as_str).eq(command.path.iter().copied()))
                .then_some((command, command.path.len()))
        })
        .max_by_key(|(_, len)| *len)
}

fn is_skipped(command: &Command, skips: &[String]) -> bool {
    skips.iter().any(|skip| {
        command.path[0] == skip
            || command.path.join(" ") == *skip
            || command.path.last().is_some_and(|name| name == skip)
    })
}

fn run_package(package: &str, args: &[String]) -> Result<()> {
    let mut cargo_args = vec!["run", "--package", package, "--"];
    cargo_args.extend(args.iter().map(String::as_str));
    cmd("cargo", cargo_args).run()?;
    Ok(())
}

fn run_dlls() -> Result<()> {
    eprintln!("warning: `cargo ci dlls` is deprecated; use `cargo regen csharp dlls` instead");
    cmd!("cargo", "regen", "csharp", "dlls").run()?;
    Ok(())
}

fn run_command(command: &Command, args: &[String]) -> Result<()> {
    if let Some(package) = command.package {
        run_package(package, args)
    } else {
        run_dlls()
    }
}

fn run_all(skip: &[String]) -> Result<()> {
    for command in COMMANDS {
        if is_skipped(command, skip) {
            continue;
        }
        run_command(command, &[])?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return run_all(&["other-workflows".to_owned()]);
    }

    if args[0] == "-h" || args[0] == "--help" {
        print_help();
        return Ok(());
    }

    if args[0] == "--skip" {
        let skip = &args[1..];
        if skip.is_empty() {
            bail!("--skip requires at least one command");
        }
        return run_all(skip);
    }

    let Some((command, consumed)) = command_for(&args) else {
        bail!("unknown cargo ci command `{}`", args[0]);
    };

    run_command(command, &args[consumed..])
}
