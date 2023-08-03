use clap::Command;
use spacetimedb_cli::*;
use spacetimedb_lib::util;
use spacetimedb_standalone::banner;

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
        .help_template(format!(
            r#"{}
Usage:
{{usage}}

Options:
{{options}}

Commands:
{{subcommands}}
"#,
            banner()
        ))
}
