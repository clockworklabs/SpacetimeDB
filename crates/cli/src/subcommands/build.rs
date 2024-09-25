use crate::Config;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use std::path::{Path, PathBuf};

pub fn cli() -> clap::Command {
    clap::Command::new("build")
        .about("Builds a spacetime module.")
        .arg(
            Arg::new("project_path")
                .long("project-path")
                .short('p')
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .help("The system path (absolute or relative) to the project you would like to build")
        )
        .arg(
            Arg::new("skip_clippy")
                .long("skip_clippy")
                .short('S')
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

pub async fn exec(_config: Config, args: &ArgMatches) -> Result<PathBuf, anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project_path").unwrap();
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

    let bin_path = crate::tasks::build(project_path, skip_clippy, build_debug)?;
    println!("Build finished successfully.");

    Ok(bin_path)
}

pub async fn exec_with_argstring(config: Config, project_path: &Path, args: &str) -> Result<PathBuf, anyhow::Error> {
    // Note: "build" must be the start of the string, because `build::cli()` is the entire build subcommand.
    // If we don't include this, the args will be misinterpreted (e.g. as commands).
    let build_options = format!("build {} --project-path {}", args, project_path.display());
    let build_args = cli().get_matches_from(build_options.split_whitespace());
    exec(config.clone(), &build_args).await
}
