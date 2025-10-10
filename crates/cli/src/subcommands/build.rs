use crate::Config;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use std::ffi::OsString;
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
            Arg::new("lint_dir")
                .long("lint-dir")
                .value_parser(clap::value_parser!(OsString))
                .default_value("src")
                .help("The directory to lint for nonfunctional print statements. If set to the empty string, skips linting.")
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .short('d')
                .action(SetTrue)
                .help("Builds the module using debug instead of release (intended to speed up local iteration, not recommended for CI)"),
        )
}

pub async fn exec(_config: Config, args: &ArgMatches) -> Result<(PathBuf, &'static str), anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project_path").unwrap();
    let lint_dir = args.get_one::<OsString>("lint_dir").unwrap();
    let lint_dir = if lint_dir.is_empty() {
        None
    } else {
        Some(PathBuf::from(lint_dir))
    };
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

    let result = crate::tasks::build(project_path, lint_dir.as_deref(), build_debug)?;
    println!("Build finished successfully.");

    Ok(result)
}

pub async fn exec_with_argstring(
    config: Config,
    project_path: &Path,
    arg_string: &str,
) -> Result<(PathBuf, &'static str), anyhow::Error> {
    // Note: "build" must be the start of the string, because `build::cli()` is the entire build subcommand.
    // If we don't include this, the args will be misinterpreted (e.g. as commands).
    let arg_string = format!("build {} --project-path {}", arg_string, project_path.display());
    let arg_matches = cli().get_matches_from(arg_string.split_whitespace());
    exec(config.clone(), &arg_matches).await
}
