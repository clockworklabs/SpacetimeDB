use crate::util::find_module_path;
use crate::Config;
use clap::ArgAction::SetTrue;
use clap::{Arg, ArgMatches};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn cli() -> clap::Command {
    clap::Command::new("build")
        .about("Builds a spacetime module.")
        .arg(
            Arg::new("module_path")
                .long("module-path")
                .short('p')
                .value_parser(clap::value_parser!(PathBuf))
                .help("The system path (absolute or relative) to the module project. Defaults to spacetimedb/ subdirectory, then current directory.")
        )
        .arg(
            Arg::new("lint_dir")
                .long("lint-dir")
                .value_parser(clap::value_parser!(OsString))
                .default_value("src")
                .help("The directory to lint for nonfunctional print statements. If set to the empty string, skips linting.")
        )
        .arg(
            // TODO: Make this into --extra-build-args (or something similar) that will get passed along to the language's compiler.
            Arg::new("features")
                .long("features")
                .value_parser(clap::value_parser!(OsString))
                .required(false)
                .help("Additional features to pass to the build process (e.g. `--features feature1,feature2` for Rust modules).")
                // We're hiding this because we think it deserves a refactor first (see the TODO above)
                .hide(true)
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
    let module_path = match args.get_one::<PathBuf>("module_path").cloned() {
        Some(path) => path,
        None => find_module_path(&std::env::current_dir()?).ok_or_else(|| {
            anyhow::anyhow!(
                "Could not find a SpacetimeDB module in spacetimedb/ or the current directory. \
                 Use --module-path to specify the module location."
            )
        })?,
    };
    let features = args.get_one::<OsString>("features");
    let lint_dir = args.get_one::<OsString>("lint_dir").unwrap();
    let lint_dir = if lint_dir.is_empty() {
        None
    } else {
        Some(PathBuf::from(lint_dir))
    };
    let build_debug = args.get_flag("debug");
    let features = features.cloned();

    run_build(module_path, lint_dir, build_debug, features)
}

pub fn run_build(
    module_path: PathBuf,
    lint_dir: Option<PathBuf>,
    build_debug: bool,
    features: Option<OsString>,
) -> Result<(PathBuf, &'static str), anyhow::Error> {
    // Create the project path, or make sure the target project path is empty.
    if module_path.exists() {
        if !module_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Fatal Error: path {} exists but is not a directory.",
                module_path.display()
            ));
        }
    } else {
        return Err(anyhow::anyhow!(
            "Fatal Error: path {} does not exist.",
            module_path.display()
        ));
    }

    let result = crate::tasks::build(&module_path, lint_dir.as_deref(), build_debug, features.as_ref())?;
    println!("Build finished successfully.");

    Ok(result)
}

pub async fn exec_with_argstring(
    project_path: &Path,
    arg_string: &str,
) -> Result<(PathBuf, &'static str), anyhow::Error> {
    let argv = exec_with_argstring_argv(project_path, arg_string);
    let arg_matches = cli().get_matches_from(argv);

    let module_path = arg_matches
        .get_one::<PathBuf>("module_path")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("module_path is required"))?;
    let features = arg_matches.get_one::<OsString>("features").cloned();
    let lint_dir = arg_matches.get_one::<OsString>("lint_dir").unwrap();
    let lint_dir = if lint_dir.is_empty() {
        None
    } else {
        Some(PathBuf::from(lint_dir))
    };
    let build_debug = arg_matches.get_flag("debug");

    run_build(module_path, lint_dir, build_debug, features)
}

fn exec_with_argstring_argv(project_path: &Path, arg_string: &str) -> Vec<OsString> {
    // Note: "build" must be the first argv token because `build::cli()` is the entire build subcommand.
    // Keep module-path as its own argv item so paths containing spaces are not split.
    let mut argv: Vec<OsString> = vec!["build".into()];
    argv.extend(arg_string.split_whitespace().map(OsString::from));
    argv.push("--module-path".into());
    argv.push(project_path.as_os_str().to_os_string());
    argv
}

#[cfg(test)]
mod tests {
    use super::exec_with_argstring_argv;
    use std::path::Path;

    #[test]
    fn exec_with_argstring_keeps_module_path_with_spaces_as_single_argv_item() {
        let project_path = Path::new("SpacetimeDB Projects/My SpacetimeDB App/spacetimedb");
        let argv = exec_with_argstring_argv(project_path, "--debug --lint-dir src");

        assert_eq!(argv[0].to_string_lossy(), "build");
        assert_eq!(argv[1].to_string_lossy(), "--debug");
        assert_eq!(argv[2].to_string_lossy(), "--lint-dir");
        assert_eq!(argv[3].to_string_lossy(), "src");
        assert_eq!(argv[4].to_string_lossy(), "--module-path");
        assert_eq!(
            argv[5].to_string_lossy(),
            "SpacetimeDB Projects/My SpacetimeDB App/spacetimedb"
        );
    }
}
