use crate::{util, Config};
use clap::{Arg, ArgMatches};
use duckscript::types::runtime::{Context, StateValue};
use std::path::Path;

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("build").about("Builds a spacetime module.").arg(
        Arg::new("project-path")
            .required(false)
            .default_value(".")
            .help("The path of the project that you would like to build."),
    )
}

pub async fn exec(_: Config, args: &ArgMatches) -> Result<(), anyhow::Error> {
    let project_path_str = args.value_of("project-path").unwrap();
    let project_path = Path::new(project_path_str);

    // Create the project path, or make sure the target project path is empty.
    if project_path.exists() {
        if !project_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Fatal Error: path {} exists but is not a directory.",
                project_path_str
            ));
        }
    } else {
        return Err(anyhow::anyhow!(
            "Fatal Error: path {} does not exist.",
            project_path_str
        ));
    }

    let mut context = Context::new();
    duckscriptsdk::load(&mut context.commands)?;
    context
        .variables
        .insert("PATH".to_string(), std::env::var("PATH").unwrap());
    context
        .variables
        .insert("PROJECT_PATH".to_string(), project_path_str.to_string());

    match util::invoke_duckscript(include_str!("project/build.duck"), context) {
        Ok(ok) => {
            let mut error = false;
            for entry in ok.state {
                if let StateValue::SubState(sub_state) = entry.1 {
                    for entry in sub_state {
                        match entry.1 {
                            StateValue::String(a) => {
                                error = true;
                                println!("{}|{}", entry.0, a)
                            }
                            _ => {}
                        }
                    }
                }
            }

            if !error {
                println!("Build finished successfully.");
            } else {
                println!("Build finished with errors, check the console for more information.");
            }
        }
        Err(e) => {
            println!("Script execution error: {}", e);
        }
    }

    Ok(())
}
