use anyhow::Ok;
use clap::Arg;
use clap::ArgMatches;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("query")
        .about("Runs a SQL query on the database.")
        .override_usage("stdb query -f <sql_query>")
        .arg(Arg::new("query").required(true))
        .after_help("Run `stdb help query for more detailed information.\n`")
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    let query = args.value_of("query").unwrap();

    println!("This is your query: {}", query);
    Ok(())
}
