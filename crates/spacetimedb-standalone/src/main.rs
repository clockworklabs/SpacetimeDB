mod client_api;
mod controller;
mod worker_db;

use clap::Parser;
use clap::Subcommand;
use spacetimedb::db::db_metrics;
use spacetimedb::startup;
use spacetimedb::worker_metrics;
use std::error::Error;
use std::net::ToSocketAddrs;
use tokio::runtime::Builder;

#[derive(Debug, Clone)]
pub struct Options {
    pub listen_addr: Option<String>,
    pub advertise_addr: Option<String>,
}

impl Options {
    const DEFAULT_PORT: u16 = 80;
    pub fn normalize(&mut self) {
        if self.advertise_addr.is_none() {
            if self.listen_addr.is_none() {
                let hostname = hostname::get().unwrap().to_str().unwrap().to_owned();
                let addr = format!("{}:{}", hostname, Self::DEFAULT_PORT);
                let _ = addr.to_socket_addrs().expect("resolve hostname");
                self.advertise_addr = Some(addr);
            } else {
                self.advertise_addr = self.listen_addr.clone();
            }
        }

        if self.listen_addr.is_none() {
            self.listen_addr = Some(format!("0.0.0.0:{}", Self::DEFAULT_PORT));
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub advertise_addr: String,
}

impl Config {
    pub fn from_options(mut options: Options) -> Self {
        options.normalize();

        Self {
            listen_addr: options.listen_addr.unwrap(),
            advertise_addr: options.advertise_addr.unwrap(),
        }
    }
}

async fn start(options: Options) -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = Config::from_options(options);

    startup::configure_logging();

    // Metrics for pieces under worker_node/ related to reducer hosting, etc.
    worker_metrics::register_custom_metrics();

    // Metrics for our use of db/.
    db_metrics::register_custom_metrics();

    client_api::start(config.listen_addr).await;
    Ok(())
}

async fn version() -> Result<(), Box<dyn Error + Send + Sync>> {
    // e.g. kubeadm version: &version.Info{Major:"1", Minor:"24", GitVersion:"v1.24.2", GitCommit:"f66044f4361b9f1f96f0053dd46cb7dce5e990a8", GitTreeState:"clean", BuildDate:"2022-06-15T14:20:54Z", GoVersion:"go1.18.3", Compiler:"gc", Platform:"linux/arm64"}
    println!("0.0.0");
    Ok(())
}

async fn async_main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = Args::parse();
    match args.command {
        Subcommands::Start {
            advertise_addr,
            listen_addr,
        } => {
            let options = Options {
                listen_addr,
                advertise_addr,
            };
            start(options).await?
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

        #[arg(short, long)]
        listen_addr: Option<String>,
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
    // Create a multi-threaded run loop
    Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
        .unwrap();
}
