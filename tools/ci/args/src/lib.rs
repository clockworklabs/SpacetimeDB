pub mod cla_assistant {
    use clap::{ArgGroup, Args as ClapArgs, Subcommand};

    #[derive(Subcommand)]
    pub enum ClaAssistantCmd {
        /// Retries CLA Assistant if `license/cla` is the only remaining PR blocker.
        Retry(RetryArgs),

        /// Returns the `license/cla` status for a pull request or commit SHA.
        Status(StatusArgs),
    }

    #[derive(ClapArgs)]
    pub struct RetryArgs {
        /// Pull request number to check.
        #[arg(long)]
        pub pr_number: u64,

        /// Repository in `owner/name` form. Defaults to GITHUB_REPOSITORY.
        #[arg(long)]
        pub repo: Option<String>,
    }

    #[derive(ClapArgs)]
    #[command(group(
        ArgGroup::new("target")
            .required(true)
            .multiple(false)
            .args(["pr", "sha"]),
    ))]
    pub struct StatusArgs {
        /// Pull request number whose head commit should be checked.
        #[arg(long)]
        pub pr: Option<u64>,

        /// Commit SHA to check.
        #[arg(long)]
        pub sha: Option<String>,

        /// Repository in `owner/name` form. Defaults to GITHUB_REPOSITORY.
        #[arg(long)]
        pub repo: Option<String>,
    }

    #[derive(ClapArgs)]
    pub struct Args {
        #[command(subcommand)]
        pub cmd: ClaAssistantCmd,
    }
}

pub mod cli_docs {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {
        #[arg(
            long,
            long_help = "specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is located)"
        )]
        pub spacetime_path: Option<String>,
    }
}

pub mod codeowners_check {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {
        /// Git ref to compare against, usually origin/<pull request base branch>.
        #[arg(long)]
        pub base_ref: String,

        /// Pull request number to inspect for approval state.
        #[arg(long)]
        pub pr_number: u64,
    }
}

pub mod coordinate_internal_tests {
    use clap::Args as ClapArgs;

    /// Selects or starts the private workflow for a public Internal Tests run.
    #[derive(ClapArgs)]
    pub struct Args {
        /// Immutable public commit to test.
        #[arg(long)]
        pub public_sha: String,

        /// Public pull request number, when coordinating a pull request run.
        #[arg(long)]
        pub public_pr_number: Option<u64>,
    }
}

pub mod docs {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod global_json_policy {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod keynote_bench {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod lint {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod publish_checks {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod smoketests {
    use clap::{Args as ClapArgs, Subcommand};

    #[derive(ClapArgs)]
    pub struct SmoketestsArgs {
        #[command(subcommand)]
        pub cmd: Option<SmoketestCmd>,

        /// Run tests against a remote server instead of spawning local servers.
        ///
        /// When specified, tests will connect to the given URL instead of starting
        /// local server instances. Tests that require local server control (like
        /// restart tests) will be skipped.
        #[arg(long)]
        pub server: Option<String>,

        /// Use a SpacetimeAuth-issued login for remote-server tests.
        ///
        /// This is required for servers that reject direct server-issued logins for privileged operations.
        ///
        /// Optionally accepts an auth host to pass through to `spacetime login`,
        /// for example `--auth-host=https://spacetimedb.com`.
        #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "")]
        pub auth_host: Option<String>,

        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        pub dotnet: bool,

        /// Additional arguments to pass to the test runner
        #[arg(trailing_var_arg = true)]
        pub args: Vec<String>,
    }

    #[derive(Subcommand)]
    pub enum SmoketestCmd {
        /// Only build binaries without running tests
        ///
        /// Use this before running `cargo test --all` to ensure binaries are built.
        Prepare,
        CheckModList,
    }
}

pub mod test {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod typescript_test {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod update_flow {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {
        #[arg(
            long,
            long_help = "Target triple to build for, by default the current target. Used by github workflows to check the update flow on multiple platforms."
        )]
        pub target: Option<String>,

        #[arg(
            long,
            default_value = "false",
            long_help = "Whether to enable github token authentication feature when building the update binary. By default this is disabled."
        )]
        pub github_token_auth: bool,
    }
}

pub mod version_upgrade_check {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod wasm_bindings {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {}
}

pub mod workflow_watch {
    use clap::Args as ClapArgs;

    #[derive(ClapArgs)]
    pub struct Args {
        /// Repository containing the workflow run, in owner/repo form.
        #[arg(long)]
        pub repo: String,

        /// GitHub Actions workflow run ID.
        #[arg(long)]
        pub run_id: u64,

        /// Seconds to sleep between polls.
        #[arg(long, default_value_t = 30)]
        pub interval_seconds: u64,

        /// Maximum number of polls before timing out. Polls forever by default.
        #[arg(long)]
        pub max_attempts: Option<u64>,
    }
}
