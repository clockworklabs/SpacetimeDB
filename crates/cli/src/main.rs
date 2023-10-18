use clap::Command;
use mimalloc::MiMalloc;
use spacetimedb_cli::*;
use spacetimedb_lib::util;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::load();
    // Save a default version to disk
    config.save();

    let (cmd, subcommand_args) = util::match_subcommand_or_exit(get_command());
    exec_subcommand(config, &cmd, &subcommand_args).await?;

    Ok(())
}

fn get_command() -> Command {
    Command::new("spacetime")
        .args_conflicts_with_subcommands(true)
        .subcommand_required(true)
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
