use crate::Config;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use std::path::PathBuf;

pub fn cli() -> clap::Command {
    clap::Command::new("build")
        .about("Builds a spacetime module.")
        .arg(
            Arg::new("project-path")
                .default_value(".")
                .value_parser(clap::value_parser!(PathBuf))
                .help("The path of the project that you would like to build."),
        )
        .arg(
            Arg::new("skip_clippy")
                .long("skip_clippy")
                .short('s')
                .action(SetTrue)
                .env("SPACETIME_SKIP_CLIPPY")
                .value_parser(clap::builder::FalseyValueParser::new())
                .help("Skips running clippy on the module before building (intended to speed up local iteration, not recommended for CI)"),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .short('d')
                .action(SetTrue)
                .help("Builds the module using debug instead of release (intended to speed up local iteration, not recommended for CI)"),
        )
}

pub async fn exec(_: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();
    let skip_clippy = args.get_flag("skip_clippy");
    let build_debug = args.get_flag("debug");

    // Create the project path, or make sure the target project path is empty.
    if project_path.exists() {
        if !project_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Fatal Error: path {} exists but is not a directory.",
                project_path.display()
            ));
        }
    } else {
        return Err(anyhow::anyhow!(
            "Fatal Error: path {} does not exist.",
            project_path.display()
        ));
    }

    crate::tasks::build(project_path, skip_clippy, build_debug)?;
    println!("Build finished successfully.");

    Ok(())
}
