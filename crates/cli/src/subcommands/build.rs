use crate::Config;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use std::path::PathBuf;

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
            Arg::new("debug")
                .long("debug")
                .short('d')
                .action(SetTrue)
                .help("Builds the module using debug instead of release (intended to speed up local iteration, not recommended for CI)"),
        )
}

pub async fn exec(config: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    // Release the lockfile on the config, since we don't need it.
    config.release_lock();

    let project_path = args.get_one::<PathBuf>("project_path").unwrap();
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

    crate::tasks::build(project_path, build_debug)?;
    println!("Build finished successfully.");

    Ok(())
}
