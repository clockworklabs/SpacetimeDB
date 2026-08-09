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
