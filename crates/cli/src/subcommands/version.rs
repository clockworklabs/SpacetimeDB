use clap::{Arg, ArgAction::SetTrue, ArgMatches};

pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

use crate::config::Config;

pub fn cli() -> clap::Command {
    clap::Command::new("version")
        .about("Print the version of the command line tool")
        .after_help("Run `spacetime help version` for more detailed information.\n")
        .arg(
            Arg::new("cli")
                .short('c')
                .long("cli")
                .action(SetTrue)
                .help("Prints only the CLI version"),
        )
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // Release the lockfile on the config, since we don't need it.
    config.release_lock();

    if args.get_flag("cli") {
        println!("{}", CLI_VERSION);
        return Ok(());
    }

    println!("Path: {}", std::env::current_exe()?.display());
    println!("Commit: {}", env!("GIT_HASH"));
    println!(
        "spacetimedb tool version {}; spacetimedb-lib version {};",
        CLI_VERSION,
        spacetimedb_lib::version::spacetimedb_lib_version()
    );
    Ok(())
}
