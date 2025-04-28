use std::process::ExitCode;

use clap::{Arg, Command};
use spacetimedb_cli::*;
use spacetimedb_paths::cli::CliTomlPath;
use spacetimedb_paths::{RootDir, SpacetimePaths};

// Note that the standalone server is invoked through standaline/src/main.rs, so you will
// also want to set the allocator there.
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[cfg(target_env = "msvc")]
use mimalloc::MiMalloc;

#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg(not(feature = "markdown-docs"))]
#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    // Compute matches before loading the config, because `Config` has an observable `drop` method
    // (which deletes a lockfile),
    // and Clap calls `exit` on parse failure rather than panicing, so destructors never run.
    let matches = get_command().get_matches();
    let (cmd, subcommand_args) = matches.subcommand().unwrap();

    let root_dir = matches.get_one::<RootDir>("root_dir");
    let paths = match root_dir {
        Some(dir) => SpacetimePaths::from_root_dir(dir),
        None => SpacetimePaths::platform_defaults()?,
    };
    let cli_toml = matches
        .get_one::<CliTomlPath>("config_path")
        .cloned()
        .unwrap_or_else(|| paths.cli_config_dir.cli_toml());
    let config = Config::load(cli_toml)?;

    exec_subcommand(config, &paths, root_dir, cmd, subcommand_args).await
}

#[cfg(feature = "markdown-docs")]
#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let markdown = clap_markdown::help_markdown_command(&get_command());
    println!("{}", markdown);
    Ok(ExitCode::SUCCESS)
}

fn get_command() -> Command {
    Command::new("spacetime")
        .version(version::CLI_VERSION)
        .long_version(version::long_version())
        .arg_required_else_help(true)
        .subcommand_required(true)
        .arg(
            Arg::new("root_dir")
                .long("root-dir")
                .help("The root directory to store all spacetime files in.")
                .value_parser(clap::value_parser!(RootDir)),
        )
        .arg(
            Arg::new("config_path")
                .long("config-path")
                .help("The path to the cli.toml config file")
                .value_parser(clap::value_parser!(CliTomlPath)),
        )
        .subcommands(get_subcommands())
        .help_expected(true)
        .help_template(
            r#"
┌───────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                       │
│                                                                                                       │
│                                                                              ⢀⠔⠁                      │
│                                                                            ⣠⡞⠁                        │
│                                              ⣀⣀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣀⣀⣀⣀⣀⣀⣀⣤⣤⡴⠒    ⢀⣠⡾⠋                          │
│                                         ⢀⣤⣶⣾88888888888888888888⠿⠋    ⢀⣴8⡟⠁                           │
│                                      ⢀⣤⣾88888⡿⠿⠛⠛⠛⠛⠛⠛⠛⠛⠻⠿88888⠟⠁    ⣠⣾88⡟                             │
│                                    ⢀⣴88888⠟⠋⠁ ⣀⣤⠤⠶⠶⠶⠶⠶⠤⣤⣀ ⠉⠉⠉    ⢀⣴⣾888⡟                              │
│                                   ⣠88888⠋  ⣠⠶⠋⠉         ⠉⠙⠶⣄   ⢀⣴888888⠃                              │
│                                  ⣰8888⡟⠁ ⣰⠟⠁               ⠈⠻⣆ ⠈⢿888888                               │
│                                 ⢠8888⡟  ⡼⠁                   ⠈⢧ ⠈⢿8888⡿                               │
│                                 ⣼8888⠁ ⢸⠇                     ⠸⡇ ⠘8888⣷                               │
│                                 88888  8                       8  88888                               │
│                                 ⢿8888⡄ ⢸⡆                     ⢰⡇ ⢀8888⡟                               │
│                                 ⣾8888⣷⡀ ⢳⡀                   ⢀⡞  ⣼8888⠃                               │
│                                 888888⣷⡀ ⠹⣦⡀               ⢀⣴⠏ ⢀⣼8888⠏                                │
│                                ⢠888888⠟⠁   ⠙⠶⣄⣀         ⣀⣠⠶⠋  ⣠88888⠋                                 │
│                                ⣼888⡿⠟⠁    ⣀⣀⣀ ⠉⠛⠒⠶⠶⠶⠶⠶⠒⠛⠉ ⢀⣠⣴88888⠟⠁                                  │
│                               ⣼88⡿⠋    ⢀⣴88888⣶⣦⣤⣤⣤⣤⣤⣤⣤⣤⣶⣾88888⡿⠛⠁                                    │
│                             ⢀⣼8⠟⠁    ⣠⣶88888888888888888888⡿⠿⠛⠁                                       │
│                            ⣠⡾⠋⠁    ⠤⠞⠛⠛⠉⠉⠉⠉⠉⠉⠉⠛⠛⠛⠛⠛⠛⠛⠛⠛⠛⠉⠉                                            │
│                          ⢀⡼⠋                                                                          │
│                        ⢀⠔⠁                                                                            │
│                                                                                                       │
│                                                                                                       │
│  .d8888b.                                     888    d8b                        8888888b.  888888b.   │
│ d88P  Y88b                                    888    Y8P                        888  "Y88b 888  "88b  │
│ Y88b.                                         888                               888    888 888  .88P  │
│  "Y888b.   88888b.   8888b.   .d8888b .d88b.  888888 888 88888b.d88b.   .d88b.  888    888 8888888K.  │
│     "Y88b. 888 "88b     "88b d88P"   d8P  Y8b 888    888 888 "888 "88b d8P  Y8b 888    888 888  "Y88b │
│       "888 888  888 .d888888 888     88888888 888    888 888  888  888 88888888 888    888 888    888 │
│ Y88b  d88P 888 d88P 888  888 Y88b.   Y8b.     Y88b.  888 888  888  888 Y8b.     888  .d88P 888   d88P │
│  "Y8888P"  88888P"  "Y888888  "Y8888P "Y8888   "Y888 888 888  888  888  "Y8888  8888888P"  8888888P"  │
│            888                                                                                        │
│            888                                                                                        │
│            888                                                                                        │
│                                  "Multiplayer at the speed of light"                                  │
│                                                                                                       │
├───────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ SpacetimeDB Command Line Tool                                                                         │
│ Easily interact with a SpacetimeDB node                                                               │
│                                                                                                       │
│ Give us feedback in our Discord server:                                                               │
│    https://discord.gg/spacetimedb                                                                     │
└───────────────────────────────────────────────────────────────────────────────────────────────────────┘
Usage:
{usage}

Options:
{options}

Commands:
{subcommands}
"#,
        )
}
