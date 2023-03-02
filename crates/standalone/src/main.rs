use anyhow::Context;
use clap::Parser;
use clap::Subcommand;

use spacetimedb::db::db_metrics;
use spacetimedb::startup;
use spacetimedb::worker_metrics;
use spacetimedb_standalone::StandaloneEnv;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::runtime::Builder;

use std::panic;
use std::process;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub advertise_addr: String,
}

impl Config {
    const DEFAULT_ADDR: &str = "0.0.0.0:80";
    pub async fn new(listen_addr: String, advertise_addr: Option<String>) -> anyhow::Result<Self> {
        let advertise_addr = match advertise_addr {
            Some(a) => a,
            None if listen_addr == Self::DEFAULT_ADDR => {
                let hostname = hostname::get().unwrap().into_string().unwrap();
                let addr = hostname + ":80";
                let _ = tokio::net::lookup_host(&addr)
                    .await
                    .context("failed to resolve hostname")?;
                addr
            }
            None => listen_addr.clone(),
        };
        Ok(Self {
            listen_addr,
            advertise_addr,
        })
    }
}

async fn start(config: Config) -> anyhow::Result<Infallible> {
    startup::configure_logging();

    // Metrics for pieces under worker_node/ related to reducer hosting, etc.
    worker_metrics::register_custom_metrics();

    // Metrics for our use of db/.
    db_metrics::register_custom_metrics();

    let ctx = StandaloneEnv::init()?;
    spacetimedb_client_api::start_control(Arc::new(ctx), config.listen_addr, |_| {}).await
}

async fn version() -> anyhow::Result<()> {
    // e.g. kubeadm version: &version.Info{Major:"1", Minor:"24", GitVersion:"v1.24.2", GitCommit:"f66044f4361b9f1f96f0053dd46cb7dce5e990a8", GitTreeState:"clean", BuildDate:"2022-06-15T14:20:54Z", GoVersion:"go1.18.3", Compiler:"gc", Platform:"linux/arm64"}
    println!("0.0.0");
    Ok(())
}

async fn async_main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Subcommands::Start {
            advertise_addr,
            listen_addr,
        } => {
            let config = Config::new(listen_addr, advertise_addr).await?;
            match start(config).await? {}
        }
        Subcommands::Version => version().await?,
    }
    Ok(())
}

#[derive(Subcommand, Debug)]
enum Subcommands {
    /// Run this command in order to set up the SpacetimeDB control plane
    Start {
        /// <node-host>:<node-port>
        #[arg(short, long)]
        advertise_addr: Option<String>,

        #[arg(short, long, default_value = Config::DEFAULT_ADDR)]
        listen_addr: String,
    },
    /// Print the version of spacetime
    Version,
}

#[derive(Parser, Debug)]
#[command(author, version, long_about=None, about=r#"
┌──────────────────────────────────────────────────────────┐
│ spacetimedb                                              │
│ Run a standalone SpacetimeDB instance                    │
│                                                          │
│ Please give us feedback at:                              │
│ https://github.com/clockworklabs/SpacetimeDB/issues      │
└──────────────────────────────────────────────────────────┘
Example usage:
┌──────────────────────────────────────────────────────────┐
│ machine# spacetimedb start                               │
└──────────────────────────────────────────────────────────┘
"#)]
struct Args {
    #[clap(subcommand)]
    command: Subcommands,
}

fn main() {
    // take_hook() returns the default hook in case when a custom one is not set
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    // Create a multi-threaded run loop
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
        .unwrap();
}
