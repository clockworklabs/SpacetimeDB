mod nodes;

use clap::Parser;
use clap::Subcommand;
use futures::future::join_all;
use futures::future::OptionFuture;
use nodes::control_node;
use nodes::node_config::NodeConfig;
use nodes::node_options::NodeOptions;
use nodes::worker_node;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::startup;
use std::error::Error;
use std::panic;
use std::process;
use std::sync::Arc;
use tokio::runtime::Builder;

/// Module API (worker nodes, port 80/443): The API for manipulating Wasm SpacetimeDB modules
///     - HTTP 1.1 (/) + WebSocket + standardized protobuf/json messages + TypeDefs
///     - (gRPC eventually?, but that requires module aware code gen, so I'm not sure how to do this)
/// Cluster API (control nodes, worker node forwarding, port 26258): The API for manipulating the cluster
///     - gRPC (/)
///     - (json eventually?)
/// Consensus API (control nodes, port 26259): The API for communicating with other control nodes
///     - gRPC (/)
///     - (WebSocket eventually?)
async fn init(options: NodeOptions) -> Result<(), Box<dyn Error + Send + Sync>> {
    let config = NodeConfig::from_options(options);
    startup::configure_logging();

    let dicc = Arc::new(DatabaseInstanceContextController::new());

    startup::init_host(&worker_node::control_node_connection::DbGetter(&dicc)).await?;

    let (worker_tasks, control_tasks) = tokio::join!(
        OptionFuture::from(config.worker_node.map(|x| worker_node::start(dicc.clone(), x))),
        OptionFuture::from(config.control_node.map(|x| control_node::start(dicc, x))),
    );

    let service_handles = itertools::chain(worker_tasks, control_tasks).flatten();

    join_all(service_handles).await;
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
        Subcommands::Init {
            listen_addr,
            advertise_addr,
            worker_node,
        } => {
            let options = NodeOptions {
                control_node: true,
                worker_node,
                listen_addr,
                advertise_addr,
                worker_api_bootstrap_addrs: Vec::new(),
                client_api_bootstrap_addrs: Vec::new(),
                peer_api_listen_addr: None,
                peer_api_advertise_addr: None,
                peer_api_bootstrap_addrs: Vec::new(),
                worker_api_listen_addr: None,
                worker_api_advertise_addr: None,
                control_api_listen_addr: None,
                control_api_advertise_addr: None,
            };
            init(options).await?
        }
        Subcommands::Join {
            advertise_addr,
            listen_addr,
            bootstrap_addrs,
            control_node,
            worker_node,
        } => {
            let options = NodeOptions {
                control_node,
                worker_node,
                listen_addr,
                advertise_addr,
                worker_api_bootstrap_addrs: if let Some(bootstrap_addrs) = &bootstrap_addrs {
                    bootstrap_addrs.split(',').map(str::to_string).collect::<Vec<_>>()
                } else {
                    Vec::new()
                },
                client_api_bootstrap_addrs: if let Some(bootstrap_addrs) = &bootstrap_addrs {
                    bootstrap_addrs.split(',').map(str::to_string).collect::<Vec<_>>()
                } else {
                    Vec::new()
                },
                peer_api_listen_addr: None,
                peer_api_advertise_addr: None,
                // TODO(cloutiertyler): I think it's fine to use the bootstrap addrs here too,
                // although it could get confusing with ports
                peer_api_bootstrap_addrs: if let Some(bootstrap_addrs) = &bootstrap_addrs {
                    bootstrap_addrs.split(',').map(str::to_string).collect::<Vec<_>>()
                } else {
                    Vec::new()
                },
                worker_api_listen_addr: None,
                worker_api_advertise_addr: None,
                control_api_listen_addr: None,
                control_api_advertise_addr: None,
            };
            init(options).await?;
        }
        Subcommands::Version => version().await?,
    }
    Ok(())
}

#[derive(Subcommand, Debug)]
enum Subcommands {
    /// Run this command in order to set up the SpacetimeDB control plane
    Init {
        /// <node-host>:<node-port>
        #[arg(short, long)]
        advertise_addr: Option<String>,

        #[arg(short, long)]
        listen_addr: Option<String>,

        #[arg(short, long, default_value_t = false)]
        worker_node: bool,
    },
    /// Run this on any machine you wish to join an existing SpacetimeDB cluster
    Join {
        /// <node-host>:<node-port>
        #[arg(short, long)]
        advertise_addr: Option<String>,

        /// <node-host>:<node-port>
        #[arg(short, long)]
        listen_addr: Option<String>,

        /// <node-host-1>:<node-port>,<node-host-2>:<node-port>,...
        #[arg(short, long)]
        bootstrap_addrs: Option<String>,

        #[arg(short, long, default_value_t = false)]
        control_node: bool,

        #[arg(short, long, default_value_t = true)]
        worker_node: bool,
    },
    /// Print the version of spacetime
    Version,
}

#[derive(Parser, Debug)]
#[command(author, version, long_about=None, about=r#"
┌──────────────────────────────────────────────────────────┐
│ spacetimedb                                              │
│ Easily bootstrap a secure SpacetimeDB cluster            │
│                                                          │
│ Please give us feedback at:                              │
│ https://github.com/clockworklabs/SpacetimeDB/issues      │
└──────────────────────────────────────────────────────────┘
Example usage:
Create a two-machine cluster with one control node
(which controls the cluster), and one worker node
(where your Spacetime Modules run).
┌──────────────────────────────────────────────────────────┐
│ On the first machine:                                    │
├──────────────────────────────────────────────────────────┤
│ control# spacetime init                                  │
└──────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────┐
│ On the second machine:                                   │
├──────────────────────────────────────────────────────────┤
│ worker# spacetime join <arguments-returned-from-init>    │
└──────────────────────────────────────────────────────────┘
You can then repeat the second step on as many other machines as you like.
"#)]
struct Args {
    #[clap(subcommand)]
    command: Subcommands,
}

/// Cluster Architecture
///  
/// spacetime - spacetimedb executable responsible for starting nodes (like kubeadm or cockroach, should do everything that stdb does as well eventually)
/// stdb - lite command line interface for controlling the cluster (like kubectl or cockroach, a subset of spacetime that doesn't require the whole executable)
/// spacetime node - a process running the spacetime daemon (like kubernetes or cockroach node, can be both worker and control)
///     NOTE: Ethereum doesn't have the concept of worker/control notes, but it also has no need for scheduling since all nodes are the same
/// spacetime worker node - a spacetime node process configured to be a worker which runs spacetime modules (maybe call this a host node?)
/// spacetime control node - a spacetime node process configured to be a controller that exposes the control API
/// TODO: spacetime pubsub node - a node that manages the communication between workers and clients
///
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
