use std::env;
use std::path::Path;
use std::process::exit;
use clap::{ArgMatches, Command};
use clap::error::{ContextKind, ContextValue};

pub fn match_subcommand_or_exit(command: Command) -> (String, ArgMatches) {
    let mut command_clone = command.clone();
    let result = command.try_get_matches();
    let args = match result {
        Ok(args) => args,
        Err(e) => match e.kind() {
            clap::error::ErrorKind::MissingSubcommand => {
                let cmd = e
                    .context()
                    .find_map(|c| match c {
                        (ContextKind::InvalidSubcommand, ContextValue::String(cmd)) => {
                            Some(cmd.split_ascii_whitespace().last().unwrap())
                        }
                        _ => None,
                    })
                    .expect("The InvalidArg to be present in the context of UnknownArgument.");
                match command_clone.find_subcommand_mut(cmd) {
                    Some(subcmd) => subcmd.print_help().unwrap(),
                    None => command_clone.print_help().unwrap(),
                }
                exit(0);
            }
            _ => {
                e.exit();
            }
        },
    };
    let (cmd, subcommand_args) = args.subcommand().unwrap();
    (cmd.to_string(), subcommand_args.clone())
}

pub fn get_exe_name() -> String {
    let exe_path = env::current_exe().expect("Failed to get executable path");
            let executable_name = Path::new(&exe_path)
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
    executable_name.to_string()
}