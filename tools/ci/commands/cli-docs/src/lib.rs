use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {
    #[arg(
        long,
        long_help = "specify a custom path to the SpacetimeDB repository root (where the main Cargo.toml is located)"
    )]
    pub spacetime_path: Option<String>,
}
