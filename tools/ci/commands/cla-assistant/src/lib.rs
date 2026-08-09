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
