use crate::Config;
use clap::{Arg, ArgMatches};
use std::path::PathBuf;

pub fn cli() -> clap::Command {
    clap::Command::new("build").about("Builds a spacetime module.").arg(
        Arg::new("project-path")
            .default_value(".")
            .value_parser(clap::value_parser!(PathBuf))
            .help("The path of the project that you would like to build."),
    )
}

pub async fn exec(_: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path = args.get_one::<PathBuf>("project-path").unwrap();

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

    crate::tasks::build(project_path)?;
    println!("Build finished successfully.");

    Ok(())
}
