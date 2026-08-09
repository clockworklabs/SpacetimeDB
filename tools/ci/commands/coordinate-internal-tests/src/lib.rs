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
