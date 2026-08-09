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
