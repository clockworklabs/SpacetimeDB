use clap::{Arg, ArgAction::SetTrue, ArgMatches};

const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn cli() -> clap::Command {
    clap::Command::new("version")
        .about("Print the version of the command line tool")
        .after_help("Run `spacetimedb help version` for more detailed information.\n")
        .arg(
            Arg::new("cli")
                .short('c')
                .long("cli")
                .action(SetTrue)
                .help("Prints only the CLI version"),
        )
}

pub async fn exec(args: &ArgMatches) -> Result<(), anyhow::Error> {
    // e.g. kubeadm version: &version.Info{Major:"1", Minor:"24", GitVersion:"v1.24.2", GitCommit:"f66044f4361b9f1f96f0053dd46cb7dce5e990a8", GitTreeState:"clean", BuildDate:"2022-06-15T14:20:54Z", GoVersion:"go1.18.3", Compiler:"gc", Platform:"linux/arm64"}
    if args.get_flag("cli") {
        println!("{}", CLI_VERSION);
        return Ok(());
    }

    println!(
        "spacetimedb tool version {}; spacetimedb-lib version {};",
        CLI_VERSION,
        spacetimedb_lib::version::spacetimedb_lib_version()
    );
    Ok(())
}
