#![allow(clippy::disallowed_macros)]

use clap::{Parser, Subcommand};
mod crates_resolver;

mod targets;
use targets::{
    cpp::CppRelease, crates::CratesRelease, csharp::CSharpRelease, docker::DockerRelease, npm::NpmRelease,
    ReleaseTarget,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(bin_name = "cargo")]
struct Cli {
    #[command(subcommand)]
    command: CargoCli,
}

#[derive(Subcommand)]
enum CargoCli {
    Release(ReleaseArgs),
}

#[derive(Parser)]
struct ReleaseArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Release crates.io packages
    Crates {
        release_version: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Release NPM package
    Npm {
        release_version: String,
        #[arg(long)]
        dry_run: bool,
    },

    /// Release C# SDK (NuGet + Unity SDK)
    Csharp {
        release_version: String,
        #[arg(long)]
        dry_run: bool,
    },

    /// Release C++ bindings (subtree mirror + git tags)
    Cpp {
        release_version: String,
        #[arg(long)]
        dry_run: bool,
    },

    /// Release Docker container
    Docker {
        release_version: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Perform a release for all targets
    #[command(name = "--all")]
    All {
        release_version: String,
        /// Skip specified targets
        #[arg(long)]
        skip: Option<Vec<String>>,
        /// Perform a dry run without actually publishing
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let CargoCli::Release(release_args) = cli.command;

    let result = match &release_args.command {
        Commands::Crates {
            release_version: version,
            dry_run,
        } => {
            let target = CratesRelease::new(version.clone(), *dry_run);
            target.release()
        }
        Commands::Csharp {
            release_version: version,
            dry_run,
        } => {
            let target = CSharpRelease::new(version.clone(), *dry_run);
            target.release()
        }
        Commands::Cpp {
            release_version: version,
            dry_run,
        } => {
            let target = CppRelease::new(version.clone(), *dry_run);
            target.release()
        }
        Commands::Npm {
            release_version: version,
            dry_run,
        } => {
            let target = NpmRelease::new(version.clone(), *dry_run);
            target.release()
        }
        Commands::Docker {
            release_version: version,
            dry_run,
        } => {
            let target = DockerRelease::new(version.clone(), *dry_run);
            target.release()
        }
        Commands::All {
            release_version: version,
            skip,
            dry_run,
        } => release_all(version.clone(), skip.clone(), *dry_run),
    };

    if let Err(err) = result {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

fn release_all(version: String, skip: Option<Vec<String>>, dry_run: bool) -> Result<(), String> {
    let skip_targets = skip.unwrap_or_default();

    let targets: Vec<Box<dyn ReleaseTarget>> = vec![
        Box::new(CratesRelease::new(version.clone(), dry_run)),
        Box::new(NpmRelease::new(version.clone(), dry_run)),
        Box::new(CSharpRelease::new(version.clone(), dry_run)),
        Box::new(CppRelease::new(version.clone(), dry_run)),
        Box::new(DockerRelease::new(version, dry_run)),
    ];

    println!("Performing a full release...");

    if !skip_targets.is_empty() {
        println!("Skipping targets: {:?}", skip_targets);
    }

    if dry_run {
        println!("DRY RUN: No changes will be published");
    }

    // Make sure all skip targets are valid
    for skip in &skip_targets {
        if targets.iter().all(|t| t.name() != skip) {
            return Err(format!("Invalid skip target: {}", skip));
        }
    }

    for target in targets {
        if skip_targets.contains(&target.name().to_string()) {
            println!("Skipping {}", target.name());
            continue;
        }

        println!("Releasing {}...", target.name());
        target.release()?;
    }

    Ok(())
}
