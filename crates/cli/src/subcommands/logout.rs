use crate::Config;
use clap::{ArgMatches, Command};

pub fn cli() -> Command {
    Command::new("logout")
}

pub async fn exec(mut config: Config, _args: &ArgMatches) -> Result<(), anyhow::Error> {
    config.clear_login_tokens();
    config.save();
    Ok(())
}
